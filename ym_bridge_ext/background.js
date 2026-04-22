importScripts("mode.js", "lastfm_api.js", "delivery.js");

const EXT_NAME = "ym-mediabridge-extension";
const EXT_VERSION = "1.0.0";

const STORAGE_KEYS = {
  SETTINGS: "settings_v1",
  STATE: "state_v1"
};

const DEFAULT_SETTINGS = {
  mode: YMBridgeMode.MODE_STANDALONE,

  desktopBridge: {
    serverUrl: "http://127.0.0.1:5000/",
    autoDiscover: true,
    candidateUrls: [
      "http://127.0.0.1:5000/",
      "http://localhost:5000/",
    ],
    healthcheckIntervalMs: 15000,
    connectTimeoutMs: 4000
  },

  lastfm: {
    apiKey: "",
    apiSecret: "",
    username: "",
    password: "",
    sessionKey: ""
  },

  debugLogs: true,
  sendAlbum: true,
  sendDuration: true,
  maxQueue: 64,
  maxRetries: 10,
  baseRetryMs: 800,
  maxRetryMs: 30000
};

const SCROBBLE_AFTER_SECS = 30;

const DEFAULT_STATE = {
  activeServerUrl: null,
  health: {
    ok: false,
    checkedAt: 0,
    lastError: null
  },

  delivery: {
    mode: YMBridgeMode.MODE_DESKTOP_BRIDGE,
    activeTarget: null,
    lastDeliveryAt: 0,
    lastDeliveryError: null
  },

  lastfm: {
    connected: false,
    authMissing: true,
    sessionCheckedAt: 0,
    lastError: null,
    lastNowPlayingAt: 0,
    lastScrobbleAt: 0
  },

  queue: [],
  inflight: false,
  lastTrack: null,
  lastSuccessAt: 0,
  lastError: null,
  stats: {
    accepted: 0,
    delivered: 0,
    failed: 0,
    retried: 0,
    droppedDuplicate: 0,
    droppedQueueFull: 0
  },
  dedupeMap: {},

  yandexTabOpen: false,
  yandexTabCount: 0,
  yandexTabSeenAt: 0,
  lastMetadataAt: 0,
  metadataActive: false,
  metadataHookEstablished: false,
  reloadLikelyNeeded: false,
  lastBrowserStatusAt: 0
};

let settings = structuredClone(DEFAULT_SETTINGS);
let runtimeState = structuredClone(DEFAULT_STATE);
let healthTimer = null;
let pumpScheduled = false;

function log(...args) {
  if (settings.debugLogs) {
    console.log("[YM-BRIDGE bg]", ...args);
  }
}

function warn(...args) {
  console.warn("[YM-BRIDGE bg]", ...args);
}

function errlog(...args) {
  console.error("[YM-BRIDGE bg]", ...args);
}

function now() {
  return Date.now();
}


function dedupeKey(payload) {
  return [
    payload.artist || "",
    payload.track || "",
    payload.album || "",
    payload.cover_url || "",
    String(payload.duration ?? "")
  ].join(" | ");
}

function standaloneTrackKey(input) {
  return [
    String(input?.artist || "").trim(),
    String(input?.track || "").trim(),
    String(input?.album || "").trim()
  ].join(" | ");
}

function standaloneScrobbleKey(item) {
  return `${standaloneTrackKey(item)} | ${String(item?.started_at || 0)}`;
}

function pruneDedupeMap() {
  const cutoff = now() - 5 * 60 * 1000;
  for (const [key, ts] of Object.entries(runtimeState.dedupeMap)) {
    if (typeof ts !== "number" || ts < cutoff) {
      delete runtimeState.dedupeMap[key];
    }
  }
}

async function refreshYandexTabState() {
  let tabs = [];

  try {
    tabs = await chrome.tabs.query({
      url: ["https://music.yandex.ru/*"]
    });
  } catch (err) {
    warn("tabs.query failed", String(err));
    tabs = [];
  }

  const isOpen = tabs.length > 0;
  const prevOpen = !!runtimeState.yandexTabOpen;

  runtimeState.yandexTabOpen = isOpen;
  runtimeState.yandexTabCount = tabs.length;

  if (isOpen) {
    if (!prevOpen || !runtimeState.yandexTabSeenAt) {
      runtimeState.yandexTabSeenAt = now();
    }
  } else {
    runtimeState.yandexTabSeenAt = 0;
    runtimeState.metadataHookEstablished = false;
    runtimeState.lastMetadataAt = 0;
    runtimeState.metadataActive = false;
  }
}

async function queryYandexMusicTabs() {
  try {
    const tabs = await chrome.tabs.query({
      url: ["https://music.yandex.ru/*"]
    });
    return Array.isArray(tabs) ? tabs : [];
  } catch (err) {
    warn("queryYandexMusicTabs failed", String(err));
    return [];
  }
}

function pickBestYandexTab(tabs) {
  if (!Array.isArray(tabs) || !tabs.length) return null;

  const activeTab = tabs.find(tab => tab.active);
  if (activeTab) return activeTab;

  const audibleTab = tabs.find(tab => tab.audible);
  if (audibleTab) return audibleTab;

  return tabs[0];
}

async function focusChromeTab(tab) {
  if (!tab || typeof tab.id !== "number") {
    throw new Error("No tab to focus");
  }

  if (typeof tab.windowId === "number") {
    await chrome.windows.update(tab.windowId, { focused: true });
  }

  await chrome.tabs.update(tab.id, { active: true });
  return tab;
}

async function focusYandexMusicTab() {
  const tabs = await queryYandexMusicTabs();
  const best = pickBestYandexTab(tabs);

  if (!best) {
    return {
      ok: false,
      found: false,
      opened: false,
      focused: false
    };
  }

  await focusChromeTab(best);
  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();

  return {
    ok: true,
    found: true,
    opened: false,
    focused: true,
    tabId: best.id ?? null,
    windowId: best.windowId ?? null
  };
}

async function openOrFocusYandexMusic() {
  const tabs = await queryYandexMusicTabs();
  const best = pickBestYandexTab(tabs);

  if (best) {
    await focusChromeTab(best);

    await refreshYandexTabState();
    refreshBrowserDiagnostics();
    await saveState();
    await updateBadge();

    return {
      ok: true,
      found: true,
      opened: false,
      focused: true,
      tabId: best.id ?? null,
      windowId: best.windowId ?? null
    };
  }

  const created = await chrome.tabs.create({
    url: "https://music.yandex.ru/",
    active: true
  });

  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();

  return {
    ok: true,
    found: false,
    opened: true,
    focused: true,
    tabId: created?.id ?? null,
    windowId: created?.windowId ?? null
  };
}

async function reloadYandexMusicTab() {
  const tabs = await queryYandexMusicTabs();
  const best = pickBestYandexTab(tabs);

  if (!best || typeof best.id !== "number") {
    return {
      ok: false,
      found: false,
      reloaded: false
    };
  }

  await chrome.tabs.reload(best.id);

  if (typeof best.windowId === "number") {
    await chrome.windows.update(best.windowId, { focused: true });
  }

  await chrome.tabs.update(best.id, { active: true });

  runtimeState.metadataHookEstablished = false;
  runtimeState.lastMetadataAt = 0;
  runtimeState.metadataActive = false;

  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();

  return {
    ok: true,
    found: true,
    reloaded: true,
    tabId: best.id,
    windowId: best.windowId ?? null
  };
}

function refreshBrowserDiagnostics() {
  const ts = now();
  runtimeState.lastBrowserStatusAt = ts;

  runtimeState.metadataActive =
    !!runtimeState.lastMetadataAt &&
    (ts - runtimeState.lastMetadataAt) <= 30000;

  runtimeState.reloadLikelyNeeded =
    !!runtimeState.health?.ok &&
    !!runtimeState.yandexTabOpen &&
    !runtimeState.metadataHookEstablished &&
    !!runtimeState.yandexTabSeenAt &&
    (ts - runtimeState.yandexTabSeenAt) >= 10000;
}

function normalizeSettings(rawSettings) {
  const raw = rawSettings || {};

  const desktopBridge = YMBridgeDelivery.getDesktopBridgeSettings(raw, {
    serverUrl: DEFAULT_SETTINGS.desktopBridge.serverUrl,
    autoDiscover: DEFAULT_SETTINGS.desktopBridge.autoDiscover,
    candidateUrls: DEFAULT_SETTINGS.desktopBridge.candidateUrls,
    healthcheckIntervalMs: DEFAULT_SETTINGS.desktopBridge.healthcheckIntervalMs,
    connectTimeoutMs: DEFAULT_SETTINGS.desktopBridge.connectTimeoutMs
  });

  return {
    mode: YMBridgeMode.getCurrentMode(raw),

    desktopBridge,

    lastfm: {
      apiKey: String(raw?.lastfm?.apiKey || "").trim(),
      apiSecret: String(raw?.lastfm?.apiSecret || "").trim(),
      username: String(raw?.lastfm?.username || "").trim(),
      password: String(raw?.lastfm?.password || "").trim(),
      sessionKey: String(raw?.lastfm?.sessionKey || "").trim()
    },

    debugLogs: raw.debugLogs ?? DEFAULT_SETTINGS.debugLogs,
    sendAlbum: raw.sendAlbum ?? DEFAULT_SETTINGS.sendAlbum,
    sendDuration: raw.sendDuration ?? DEFAULT_SETTINGS.sendDuration,
    maxQueue: Number(raw.maxQueue ?? DEFAULT_SETTINGS.maxQueue),
    maxRetries: Number(raw.maxRetries ?? DEFAULT_SETTINGS.maxRetries),
    baseRetryMs: Number(raw.baseRetryMs ?? DEFAULT_SETTINGS.baseRetryMs),
    maxRetryMs: Number(raw.maxRetryMs ?? DEFAULT_SETTINGS.maxRetryMs)
  };
}

function normalizeState(rawState) {
  const raw = rawState || {};

  const normalized = {
    ...DEFAULT_STATE,
    ...raw,
    delivery: {
      ...DEFAULT_STATE.delivery,
      ...(raw.delivery || {})
    },
    lastfm: {
      ...DEFAULT_STATE.lastfm,
      ...(raw.lastfm || {})
    },
    health: {
      ...DEFAULT_STATE.health,
      ...(raw.health || {})
    },
    stats: {
      ...DEFAULT_STATE.stats,
      ...(raw.stats || {})
    }
  };

  if (Array.isArray(normalized.queue)) {
    normalized.queue = normalized.queue.map(item => {
      const startedAt = Number.isFinite(Number(item?.started_at))
        ? Math.floor(Number(item.started_at))
        : Math.floor(now() / 1000);

      const artist = String(item?.artist || "").trim();
      const track = String(item?.track || "").trim();
      const album = String(item?.album || "").trim();

      return {
        now_playing_sent: false,
        scrobble_sent: false,
        started_at: startedAt,
        scrobble_due_at: Number.isFinite(Number(item?.scrobble_due_at))
          ? Math.floor(Number(item.scrobble_due_at))
          : (startedAt + SCROBBLE_AFTER_SECS),
        track_key: item?.track_key || standaloneTrackKey({ artist, track, album }),
        ...item
      };
    });
  }

  return normalized;
}

async function loadAll() {
  const data = await chrome.storage.local.get([
    STORAGE_KEYS.SETTINGS,
    STORAGE_KEYS.STATE
  ]);

  settings = normalizeSettings(data[STORAGE_KEYS.SETTINGS]);
  runtimeState = normalizeState(data[STORAGE_KEYS.STATE]);

  if (!Array.isArray(runtimeState.queue)) {
    runtimeState.queue = [];
  }

  if (!runtimeState.dedupeMap || typeof runtimeState.dedupeMap !== "object") {
    runtimeState.dedupeMap = {};
  }

  runtimeState.inflight = false;
  runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);

  pruneDedupeMap();
}

function resetBrowserSessionState() {
  runtimeState.yandexTabOpen = false;
  runtimeState.yandexTabCount = 0;
  runtimeState.yandexTabSeenAt = 0;
  runtimeState.lastMetadataAt = 0;
  runtimeState.metadataActive = false;
  runtimeState.metadataHookEstablished = false;
  runtimeState.reloadLikelyNeeded = false;
  runtimeState.lastBrowserStatusAt = 0;
}

async function saveSettings() {
  await chrome.storage.local.set({
    [STORAGE_KEYS.SETTINGS]: settings
  });
}

async function saveState() {
  await chrome.storage.local.set({
    [STORAGE_KEYS.STATE]: runtimeState
  });
}

async function pushLastfmToCompanion(reason = "manual") {
  const payload = {
    schema_version: 1,
    source: "ym-mediabridge-extension",
    synced_at: now(),
    reason,
    api_key: String(settings?.lastfm?.apiKey || "").trim(),
    api_secret: String(settings?.lastfm?.apiSecret || "").trim(),
    username: String(settings?.lastfm?.username || "").trim(),
    session_key: String(settings?.lastfm?.sessionKey || "").trim()
  };

  if (!payload.api_key || !payload.api_secret || !payload.username || !payload.session_key) {
    return { ok: false, skipped: true, error: "lastfm payload incomplete" };
  }

  try {
    const resp = await fetch("http://127.0.0.1:5000/companion/import-lastfm", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify(payload)
    });

    const text = await resp.text();

    if (!resp.ok) {
      throw new Error(`HTTP ${resp.status}: ${text}`);
    }

    log("pushLastfmToCompanion OK", { reason, text });
    return { ok: true, skipped: false };
  } catch (err) {
    log("pushLastfmToCompanion skipped", { reason, error: String(err) });
    return { ok: false, skipped: true, error: String(err) };
  }
}

async function importLastfmFromCompanion(reason = "manual-import") {
  const resp = await fetch("http://127.0.0.1:5000/companion/export-lastfm", {
    method: "GET"
  });

  const text = await resp.text();

  if (!resp.ok) {
    throw new Error(`HTTP ${resp.status}: ${text}`);
  }

  let parsed = null;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error(`Invalid JSON from companion: ${text}`);
  }

  if (!parsed?.ok) {
    throw new Error("Desktop companion returned empty Last.fm data");
  }

  settings.lastfm = {
    ...(settings.lastfm || {}),
    apiKey: String(parsed.api_key || "").trim(),
    apiSecret: String(parsed.api_secret || "").trim(),
    username: String(parsed.username || "").trim(),
    password: String(settings?.lastfm?.password || "").trim(),
    sessionKey: String(parsed.session_key || "").trim()
  };

  runtimeState.lastfm = {
    ...runtimeState.lastfm,
    connected: !!String(parsed.session_key || "").trim(),
    authMissing: !String(parsed.session_key || "").trim(),
    sessionCheckedAt: now(),
    lastError: null
  };

  await saveSettings();
  await saveState();
  await updateBadge();

  log("importLastfmFromCompanion OK", {
    reason,
    username: settings.lastfm.username,
    hasSessionKey: !!settings.lastfm.sessionKey
  });

  return buildPublicState();
}

const ICON_PATHS_COLOR = {
  16: "icons/icon16.png",
  32: "icons/icon32.png",
  48: "icons/icon48.png",
  128: "icons/icon128.png"
};

const ICON_PATHS_BW = {
  16: "icons/icon16_bw.png",
  32: "icons/icon32_bw.png",
  48: "icons/icon48_bw.png",
  128: "icons/icon128_bw.png"
};

async function updateActionIcon() {
  const useBw =
    !runtimeState.health?.ok ||
    runtimeState.reloadLikelyNeeded ||
    !runtimeState.yandexTabOpen;

  await chrome.action.setIcon({
    path: useBw ? ICON_PATHS_BW : ICON_PATHS_COLOR
  });
}

function resolveActionStatus() {
  const mode = YMBridgeMode.getCurrentMode(settings);

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    if (!runtimeState.health?.ok) {
      return {
        badgeText: "!",
        badgeColor: "#a12a2a",
        title: `YM Bridge\nMode: ${mode}\nDesktop app: not detected\n${runtimeState.health?.lastError || "No reachable localhost endpoint"}`
      };
    }

    if (runtimeState.reloadLikelyNeeded) {
      return {
        badgeText: "!",
        badgeColor: "#b36b00",
        title: `YM Bridge\nMode: ${mode}\nDesktop app: connected\nStatus: Reload Yandex Music tab`
      };
    }

    if (!runtimeState.yandexTabOpen) {
      return {
        badgeText: "",
        badgeColor: null,
        title: `YM Bridge\nMode: ${mode}\nDesktop app: connected\nStatus: Open Yandex Music`
      };
    }

    if (runtimeState.metadataActive) {
      return {
        badgeText: "",
        badgeColor: null,
        title: `YM Bridge\nMode: ${mode}\nDesktop app: connected\nStatus: Connected`
      };
    }

    return {
      badgeText: "",
      badgeColor: null,
      title: `YM Bridge\nMode: ${mode}\nDesktop app: connected\nStatus: Waiting for track metadata`
    };
  }

  const hasRuntimeAuth = YMBridgeLastfmApi.hasStandaloneRuntimeAuth(settings?.lastfm);
  const hasFullConfig = YMBridgeLastfmApi.isLastfmConfigComplete(settings?.lastfm);

  if (!hasRuntimeAuth) {
    return {
      badgeText: "!",
      badgeColor: "#a12a2a",
      title: `YM Bridge\nMode: ${mode}\nStatus: Last.fm auth missing`
    };
  }

  if (runtimeState.lastfm?.lastError) {
    return {
      badgeText: "!",
      badgeColor: "#a12a2a",
      title: `YM Bridge\nMode: ${mode}\nLast.fm error: ${runtimeState.lastfm.lastError}`
    };
  }

  if (!runtimeState.yandexTabOpen) {
    return {
      badgeText: "",
      badgeColor: null,
      title: `YM Bridge\nMode: ${mode}\nStatus: Open Yandex Music`
    };
  }

  if (runtimeState.reloadLikelyNeeded) {
    return {
      badgeText: "!",
      badgeColor: "#b36b00",
      title: `YM Bridge\nMode: ${mode}\nStatus: Reload Yandex Music tab`
    };
  }

  const pendingStandalone = Array.isArray(runtimeState.queue)
    ? runtimeState.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
    : false;
  if (pendingStandalone) {
    return {
      badgeText: "",
      badgeColor: null,
      title: `YM Bridge\nMode: ${mode}\nStatus: Scrobble pending`
    };
  }

  if (runtimeState.metadataActive) {
    return {
      badgeText: "",
      badgeColor: null,
      title: `YM Bridge\nMode: ${mode}\nStatus: Connected`
    };
  }

  return {
    badgeText: "",
    badgeColor: null,
    title: `YM Bridge\nMode: ${mode}\nStatus: Waiting for track metadata`
  };
}

async function updateBadge() {
  const status = resolveActionStatus();

  await updateActionIcon();
  await chrome.action.setBadgeText({ text: status.badgeText });

  if (status.badgeColor) {
    await chrome.action.setBadgeBackgroundColor({ color: status.badgeColor });
  }

  await chrome.action.setTitle({
    title: status.title
  });
}

function buildTrackEnvelope(payload, sender) {
  const startedAt = Math.floor(now() / 1000);
  const artist = String(payload.artist || "").trim();
  const track = String(payload.track || "").trim();
  const album = settings.sendAlbum ? String(payload.album || "").trim() : "";

  return {
    schema_version: 1,
    client_name: EXT_NAME,
    client_version: EXT_VERSION,
    event_type: "track_metadata",
    event_id: payload.event_id,
    sent_at: now(),
    page_ts: payload.ts ?? null,
    page_url: payload.page_url ?? null,
    page_href: payload.page_url ?? null,
    reason: payload.reason ?? null,
    artist,
    track,
    album,
    cover_url: String(payload.cover_url || "").trim(),
    duration: settings.sendDuration ? (Number.isFinite(payload.duration) ? payload.duration : null) : null,
    started_at: startedAt,
    scrobble_due_at: startedAt + SCROBBLE_AFTER_SECS,
    track_key: standaloneTrackKey({ artist, track, album }),
    now_playing_sent: false,
    scrobble_sent: false,
    tab_id: sender?.tab?.id ?? null,
    frame_id: sender?.frameId ?? null,
    retry_count: 0,
    dedupe_key: dedupeKey(payload)
  };
}

function validateEnvelope(envelope) {
  if (!envelope.artist || !envelope.track) {
    return { ok: false, error: "artist/track missing" };
  }

  if (envelope.artist.length > 300 || envelope.track.length > 300 || envelope.album.length > 300) {
    return { ok: false, error: "field too long" };
  }

  if (envelope.cover_url && !/^https?:\/\//i.test(envelope.cover_url)) {
    envelope.cover_url = "";
  }

  if (envelope.duration != null) {
    if (!Number.isFinite(envelope.duration) || envelope.duration <= 0 || envelope.duration > 24 * 60 * 60) {
      envelope.duration = null;
    }
  }

  return { ok: true };
}

function enqueue(envelope) {
  pruneDedupeMap();

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    if (runtimeState.dedupeMap[envelope.dedupe_key]) {
      runtimeState.stats.droppedDuplicate += 1;
      return { ok: true, queued: false, duplicate: true };
    }

    const alreadyQueued = runtimeState.queue.some(item => item.dedupe_key === envelope.dedupe_key);
    if (alreadyQueued) {
      runtimeState.stats.droppedDuplicate += 1;
      return { ok: true, queued: false, duplicate: true };
    }
  } else {
    const activeStandaloneItem = runtimeState.queue.find(item =>
      item.track_key === envelope.track_key &&
      !item.scrobble_sent
    );

    if (activeStandaloneItem) {
      activeStandaloneItem.cover_url = envelope.cover_url || activeStandaloneItem.cover_url;
      activeStandaloneItem.duration = envelope.duration ?? activeStandaloneItem.duration;
      activeStandaloneItem.page_url = envelope.page_url ?? activeStandaloneItem.page_url;
      activeStandaloneItem.page_href = envelope.page_href ?? activeStandaloneItem.page_href;
      activeStandaloneItem.sent_at = envelope.sent_at;
      activeStandaloneItem.reason = envelope.reason ?? activeStandaloneItem.reason;

      runtimeState.lastTrack = {
        artist: activeStandaloneItem.artist,
        track: activeStandaloneItem.track,
        album: activeStandaloneItem.album,
        cover_url: activeStandaloneItem.cover_url,
        duration: activeStandaloneItem.duration,
        page_url: activeStandaloneItem.page_url,
        started_at: activeStandaloneItem.started_at,
        scrobble_due_at: activeStandaloneItem.scrobble_due_at,
        now_playing_sent: !!activeStandaloneItem.now_playing_sent,
        scrobble_sent: !!activeStandaloneItem.scrobble_sent,
        updated_at: now()
      };

      runtimeState.stats.droppedDuplicate += 1;

      return {
        ok: true,
        queued: false,
        duplicate: true,
        queueLength: runtimeState.queue.length
      };
    }
  }

  const maxQueue = Math.max(1, Number(settings.maxQueue) || DEFAULT_SETTINGS.maxQueue);
  if (runtimeState.queue.length >= maxQueue) {
    runtimeState.queue.shift();
    runtimeState.stats.droppedQueueFull += 1;
  }

  runtimeState.queue.push(envelope);
  runtimeState.lastTrack = {
    artist: envelope.artist,
    track: envelope.track,
    album: envelope.album,
    cover_url: envelope.cover_url,
    duration: envelope.duration,
    page_url: envelope.page_url,
    started_at: envelope.started_at,
    scrobble_due_at: envelope.scrobble_due_at,
    now_playing_sent: !!envelope.now_playing_sent,
    scrobble_sent: !!envelope.scrobble_sent,
    updated_at: now()
  };
  runtimeState.stats.accepted += 1;

  return {
    ok: true,
    queued: true,
    duplicate: false,
    queueLength: runtimeState.queue.length
  };
}

function retryDelayMs(retryCount) {
  const base = Math.max(100, Number(settings.baseRetryMs) || DEFAULT_SETTINGS.baseRetryMs);
  const max = Math.max(base, Number(settings.maxRetryMs) || DEFAULT_SETTINGS.maxRetryMs);
  const raw = Math.min(base * Math.pow(2, retryCount), max);
  const jitter = Math.floor(Math.random() * 250);
  return raw + jitter;
}

async function schedulePump(delay = 0) {
  if (pumpScheduled) return;
  pumpScheduled = true;

  setTimeout(async () => {
    pumpScheduled = false;
    await pumpQueue();
  }, delay);
}

async function pumpQueue() {
  if (runtimeState.inflight) return;
  if (!runtimeState.queue.length) {
    await updateBadge();
    return;
  }

  runtimeState.inflight = true;
  await saveState();

  try {
    while (runtimeState.queue.length) {
      const item = runtimeState.queue[0];

      try {
        log("POST START", {
          event_id: item.event_id,
          artist: item.artist,
          track: item.track,
          retry_count: item.retry_count
        });

        const result = await YMBridgeDelivery.deliverEnvelope(
          item,
          runtimeState,
          settings,
          DEFAULT_SETTINGS.desktopBridge,
          {
            extName: EXT_NAME,
            extVersion: EXT_VERSION,
            saveState,
            saveSettings,
            updateBadge,
            log,
            warn,
            syncCompanionLastfm: pushLastfmToCompanion
          }
        );

        runtimeState.lastSuccessAt = now();
        runtimeState.lastError = null;

        if (result?.done === false) {
          if (runtimeState.lastTrack && runtimeState.lastTrack.track === item.track && runtimeState.lastTrack.artist === item.artist) {
            runtimeState.lastTrack.now_playing_sent = !!item.now_playing_sent;
            runtimeState.lastTrack.scrobble_sent = !!item.scrobble_sent;
            runtimeState.lastTrack.started_at = item.started_at;
            runtimeState.lastTrack.scrobble_due_at = item.scrobble_due_at;
            runtimeState.lastTrack.updated_at = now();
          }

          log("POST DEFERRED", {
            event_id: item.event_id,
            nextDelay: result.nextDelay,
            now_playing_sent: item.now_playing_sent,
            scrobble_sent: item.scrobble_sent,
            started_at: item.started_at,
            scrobble_due_at: item.scrobble_due_at
          });

          await saveState();
          await updateBadge();
          await schedulePump(Math.max(1000, Number(result.nextDelay) || 1000));
          return;
        }

        runtimeState.queue.shift();

        if (YMBridgeMode.isStandaloneMode(settings)) {
          runtimeState.dedupeMap[standaloneScrobbleKey(item)] = now();
        } else {
          runtimeState.dedupeMap[item.dedupe_key] = now();
        }

        runtimeState.stats.delivered += 1;

        if (runtimeState.lastTrack && runtimeState.lastTrack.track === item.track && runtimeState.lastTrack.artist === item.artist) {
          runtimeState.lastTrack.now_playing_sent = true;
          runtimeState.lastTrack.scrobble_sent = true;
          runtimeState.lastTrack.started_at = item.started_at;
          runtimeState.lastTrack.scrobble_due_at = item.scrobble_due_at;
          runtimeState.lastTrack.updated_at = now();
        }

        log("POST OK", {
          event_id: item.event_id,
          status: result.status,
          text: result.text,
          json: result.json
        });

        await saveState();
        await updateBadge();
      } catch (err) {
        runtimeState.stats.failed += 1;
        runtimeState.lastError = String(err);
        runtimeState.health = {
          ok: false,
          checkedAt: now(),
          lastError: String(err)
        };

        const current = runtimeState.queue[0];
        current.retry_count += 1;

        if (current.retry_count > (Number(settings.maxRetries) || DEFAULT_SETTINGS.maxRetries)) {
          warn("DROP after max retries", {
            event_id: current.event_id,
            error: String(err)
          });
          runtimeState.queue.shift();
          await saveState();
          await updateBadge();
          continue;
        }

        const delay = retryDelayMs(current.retry_count);
        runtimeState.stats.retried += 1;

        warn("POST RETRY", {
          event_id: current.event_id,
          retry_count: current.retry_count,
          delay,
          error: String(err)
        });

        await saveState();
        await updateBadge();
        await schedulePump(delay);
        return;
      }
    }
  } finally {
    runtimeState.inflight = false;
    await saveState();
    await updateBadge();
  }
}

async function applySettings(newSettings) {
  settings = normalizeSettings({
    ...settings,
    ...(newSettings || {})
  });

  runtimeState.delivery = runtimeState.delivery || {};
  runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);

  await saveSettings();

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    await YMBridgeDelivery.discoverDesktopBridge(
      runtimeState,
      settings,
      DEFAULT_SETTINGS.desktopBridge,
      {
        extName: EXT_NAME,
        extVersion: EXT_VERSION,
        saveState,
        updateBadge,
        log,
        warn,
        syncCompanionLastfm: pushLastfmToCompanion
      }
    );

    try {
      await YMBridgeDelivery.sendExtensionPing(
        runtimeState,
        settings,
        DEFAULT_SETTINGS.desktopBridge,
        {
          extName: EXT_NAME,
          extVersion: EXT_VERSION,
          saveState,
          updateBadge,
          log,
          warn,
          syncCompanionLastfm: pushLastfmToCompanion
        }
      );
    } catch (e) {
      log("extension ping after settings apply failed", String(e));
    }
  } else {
    runtimeState.activeServerUrl = null;
    runtimeState.delivery = runtimeState.delivery || {};
    runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);
    runtimeState.delivery.activeTarget = "lastfm";
    runtimeState.delivery.lastDeliveryError = null;

    runtimeState.health = {
      ok: true,
      checkedAt: now(),
      lastError: null
    };

    runtimeState.lastfm = runtimeState.lastfm || {};
    runtimeState.lastfm.authMissing =
      !YMBridgeLastfmApi.isLastfmConfigComplete(settings?.lastfm) ||
      !String(settings?.lastfm?.sessionKey || "").trim();
  }

  await saveState();
  await updateBadge();
  await pumpQueue();
}

function buildPublicState() {
  return {
    extension: {
      name: EXT_NAME,
      version: EXT_VERSION
    },
    settings,
    runtime: {
      activeServerUrl: runtimeState.activeServerUrl,
      health: runtimeState.health,
      delivery: runtimeState.delivery,
      lastfm: runtimeState.lastfm,
      queueLength: runtimeState.queue.length,
      inflight: runtimeState.inflight,
      lastTrack: runtimeState.lastTrack,
      lastSuccessAt: runtimeState.lastSuccessAt,
      lastError: runtimeState.lastError,
      stats: runtimeState.stats,
      browser: {
        yandexTabOpen: runtimeState.yandexTabOpen,
        yandexTabCount: runtimeState.yandexTabCount,
        yandexTabSeenAt: runtimeState.yandexTabSeenAt,
        lastMetadataAt: runtimeState.lastMetadataAt,
        metadataActive: runtimeState.metadataActive,
        metadataHookEstablished: runtimeState.metadataHookEstablished,
        reloadLikelyNeeded: runtimeState.reloadLikelyNeeded,
        lastBrowserStatusAt: runtimeState.lastBrowserStatusAt
      }
    }
  };
}

function buildEnhancedState() {
  const publicState = buildPublicState();
  const runtime = publicState.runtime;
  const browser = runtime.browser || {};
  const mode = YMBridgeMode.getCurrentMode(settings);

  let status = "unknown";
  let hint = "Unknown state";

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    if (!runtime.health?.ok) {
      status = "desktop-app-not-detected";
      hint = "Start the desktop companion or switch to standalone mode.";
    } else if (browser.reloadLikelyNeeded) {
      status = "reload-yandex-music-tab";
      hint = "Reload the Yandex Music tab.";
    } else if (!browser.yandexTabOpen) {
      status = "open-yandex-music";
      hint = "Open music.yandex.ru.";
    } else if (browser.metadataActive) {
      status = "connected";
      hint = "Desktop companion bridge is healthy.";
    } else {
      status = "waiting-for-track-metadata";
      hint = "Waiting for Yandex Music metadata.";
    }
  } else {
    const hasSessionKey = !!String(settings?.lastfm?.sessionKey || "").trim();
    const hasFullConfig = YMBridgeLastfmApi.isLastfmConfigComplete(settings?.lastfm);
    const pendingStandalone = Array.isArray(runtimeState.queue)
      ? runtimeState.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
      : false;

    if (!hasFullConfig) {
      status = "lastfm-auth-missing";
      hint = "Open settings and fill in Last.fm credentials.";
    } else if (!hasSessionKey) {
      status = "lastfm-auth-missing";
      hint = "Validate Last.fm and save the session key.";
    } else if (runtime.lastfm?.lastError) {
      status = "lastfm-error";
      hint = runtime.lastfm.lastError;
    } else if (!browser.yandexTabOpen) {
      status = "open-yandex-music";
      hint = "Open music.yandex.ru.";
    } else if (browser.reloadLikelyNeeded) {
      status = "reload-yandex-music-tab";
      hint = "Reload the Yandex Music tab.";
    } else if (pendingStandalone) {
      status = "scrobble-pending";
      hint = "Now Playing sent. Waiting before scrobble.";
    } else if (browser.metadataActive && runtime.lastfm?.connected) {
      status = "connected";
      hint = "Direct Last.fm delivery is healthy.";
    } else if (browser.metadataActive) {
      status = "connected";
      hint = "Track metadata is flowing.";
    } else {
      status = "waiting-for-track-metadata";
      hint = "Waiting for Yandex Music metadata.";
    }
  }

  return {
    ...publicState,
    ui: {
      status,
      hint,
      mode,
      actions: {
        canOpenOrFocusYandexMusic: true,
        canFocusYandexMusicTab: !!browser.yandexTabOpen,
        canReloadYandexMusicTab: !!browser.yandexTabOpen,
        canOpenSettings: true,
        canOpenDesktopAppPage: YMBridgeMode.isDesktopBridgeMode(settings)
      }
    }
  };
}


function startHealthTimer() {
  if (healthTimer) {
    clearInterval(healthTimer);
  }

  const desktop = settings.desktopBridge || DEFAULT_SETTINGS.desktopBridge;
  const interval = Math.max(
    3000,
    Number(desktop.healthcheckIntervalMs) || DEFAULT_SETTINGS.desktopBridge.healthcheckIntervalMs
  );

  healthTimer = setInterval(async () => {
    if (YMBridgeMode.isDesktopBridgeMode(settings)) {
      await YMBridgeDelivery.ensureDesktopBridge(
        runtimeState,
        settings,
        DEFAULT_SETTINGS.desktopBridge,
        {
          extName: EXT_NAME,
          extVersion: EXT_VERSION,
          saveState,
          updateBadge,
          log,
          warn,
          syncCompanionLastfm: pushLastfmToCompanion
        }
      );

      try {
        await YMBridgeDelivery.sendExtensionPing(
          runtimeState,
          settings,
          DEFAULT_SETTINGS.desktopBridge,
          {
            extName: EXT_NAME,
            extVersion: EXT_VERSION,
            saveState,
            updateBadge,
            log,
            warn,
            syncCompanionLastfm: pushLastfmToCompanion
          }
        );
      } catch (e) {
        log("periodic extension ping failed", String(e));
      }
    }

    await refreshYandexTabState();
    refreshBrowserDiagnostics();
    await saveState();
    await updateBadge();
  }, interval);
}

chrome.runtime.onInstalled.addListener(async () => {
  await loadAll();
  resetBrowserSessionState();
  await saveSettings();

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    await YMBridgeDelivery.discoverDesktopBridge(
      runtimeState,
      settings,
      DEFAULT_SETTINGS.desktopBridge,
      {
        extName: EXT_NAME,
        extVersion: EXT_VERSION,
        saveState,
        updateBadge,
        log,
        warn,
        syncCompanionLastfm: pushLastfmToCompanion
      }
    );

    try {
      await YMBridgeDelivery.sendExtensionPing(
        runtimeState,
        settings,
        DEFAULT_SETTINGS.desktopBridge,
        {
          extName: EXT_NAME,
          extVersion: EXT_VERSION,
          saveState,
          updateBadge,
          log,
          warn,
          syncCompanionLastfm: pushLastfmToCompanion
        }
      );
    } catch (e) {
      log("startup extension ping failed", String(e));
    }
  } else {
    runtimeState.activeServerUrl = null;
    runtimeState.health = {
      ok: true,
      checkedAt: now(),
      lastError: null
    };

    runtimeState.delivery = runtimeState.delivery || {};
    runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);
    runtimeState.delivery.activeTarget = "lastfm";
    runtimeState.delivery.lastDeliveryError = null;
  }

  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();
  startHealthTimer();
});

chrome.runtime.onStartup.addListener(async () => {
  await loadAll();
  resetBrowserSessionState();

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    await YMBridgeDelivery.discoverDesktopBridge(
      runtimeState,
      settings,
      DEFAULT_SETTINGS.desktopBridge,
      {
        extName: EXT_NAME,
        extVersion: EXT_VERSION,
        saveState,
        updateBadge,
        log,
        warn,
        syncCompanionLastfm: pushLastfmToCompanion
      }
    );

    try {
      await YMBridgeDelivery.sendExtensionPing(
        runtimeState,
        settings,
        DEFAULT_SETTINGS.desktopBridge,
        {
          extName: EXT_NAME,
          extVersion: EXT_VERSION,
          saveState,
          updateBadge,
          log,
          warn,
          syncCompanionLastfm: pushLastfmToCompanion
        }
      );
    } catch (e) {
      log("startup extension ping failed", String(e));
    }
  } else {
    runtimeState.activeServerUrl = null;
    runtimeState.health = {
      ok: true,
      checkedAt: now(),
      lastError: null
    };

    runtimeState.delivery = runtimeState.delivery || {};
    runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);
    runtimeState.delivery.activeTarget = "lastfm";
    runtimeState.delivery.lastDeliveryError = null;
  }

  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();
  startHealthTimer();
  await pumpQueue();
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    try {
      if (!message || typeof message !== "object") {
        sendResponse({ ok: false, error: "bad message" });
        return;
      }

      if (message.type === "metadata-heartbeat") {
        const payload = message.payload || {};

        const artist = String(payload.artist || "").trim();
        const track = String(payload.track || "").trim();

        if (!artist || !track) {
          sendResponse({ ok: false, error: "artist/track missing" });
          return;
        }

        runtimeState.lastMetadataAt = now();
        runtimeState.metadataHookEstablished = true;

        if (!runtimeState.lastTrack) {
          runtimeState.lastTrack = {
            artist,
            track,
            album: String(payload.album || "").trim(),
            cover_url: String(payload.cover_url || "").trim(),
            duration: Number.isFinite(payload.duration) ? payload.duration : null,
            page_url: payload.page_url ?? null,
            updated_at: now()
          };
        } else {
          runtimeState.lastTrack.updated_at = now();
        }

        await refreshYandexTabState();
        refreshBrowserDiagnostics();
        await saveState();
        await updateBadge();

        sendResponse({
          ok: true,
          alive: true,
          queueLength: runtimeState.queue.length,
          activeServerUrl: runtimeState.activeServerUrl
        });
        return;
      }

      if (message.type === "post-metadata") {
        const envelope = buildTrackEnvelope(message.payload || {}, sender);
        const validation = validateEnvelope(envelope);

        if (!validation.ok) {
          sendResponse({ ok: false, error: validation.error });
          return;
        }

        const result = enqueue(envelope);

        runtimeState.lastMetadataAt = now();
        runtimeState.metadataHookEstablished = true;
        await refreshYandexTabState();
        refreshBrowserDiagnostics();

        await saveState();
        await updateBadge();
        await schedulePump(0);

        sendResponse({
          ok: true,
          ...result,
          queueLength: runtimeState.queue.length,
          activeServerUrl: runtimeState.activeServerUrl
        });
        return;
      }

      if (message.type === "get-state") {
        sendResponse({ ok: true, data: buildPublicState() });
        return;
      }

      if (message.type === "retry-now") {
        await YMBridgeDelivery.retryNow(
          runtimeState,
          settings,
          DEFAULT_SETTINGS.desktopBridge,
          {
            extName: EXT_NAME,
            extVersion: EXT_VERSION,
            saveState,
            updateBadge,
            log,
            warn,
            syncCompanionLastfm: pushLastfmToCompanion
          }
        );

        await pumpQueue();
        sendResponse({ ok: true, data: buildPublicState() });
        return;
      }

      if (message.type === "save-settings") {
        await applySettings(message.settings || {});
        await pushLastfmToCompanion("save-settings");
        sendResponse({ ok: true, data: buildPublicState() });
        return;
      }

      if (message.type === "validate-lastfm") {
        try {
          log("validate-lastfm START", {
            username: message?.lastfm?.username || "",
            hasApiKey: !!message?.lastfm?.apiKey,
            hasApiSecret: !!message?.lastfm?.apiSecret,
            hasPassword: !!message?.lastfm?.password
          });

          const lastfmInput = message.lastfm || settings.lastfm || {};
          const result = await YMBridgeLastfmApi.validateCredentials(lastfmInput);

          settings.lastfm = {
            ...(settings.lastfm || {}),
            ...lastfmInput,
            sessionKey: result.sessionKey
          };

          await saveSettings();
          await pushLastfmToCompanion("validate-lastfm");

          log("validate-lastfm OK", {
            username: lastfmInput?.username || "",
            hasSessionKey: !!result?.sessionKey
          });

          sendResponse({
            ok: true,
            data: {
              sessionKey: result.sessionKey
            }
          });
        } catch (err) {
          errlog("validate-lastfm FAILED", err);
          sendResponse({
            ok: false,
            error: String(err)
          });
        }
        return;
      }

      if (message.type === "import-lastfm-from-companion") {
        try {
          const data = await importLastfmFromCompanion("options-import");
          sendResponse({ ok: true, data });
        } catch (err) {
          sendResponse({ ok: false, error: String(err) });
        }
        return;
      }

      if (message.type === "clear-lastfm-session") {
        settings.lastfm = {
          ...(settings.lastfm || {}),
          sessionKey: ""
        };

        runtimeState.lastfm = {
          ...runtimeState.lastfm,
          connected: false,
          authMissing: true,
          sessionCheckedAt: now(),
          lastError: null,
          lastNowPlayingAt: 0,
          lastScrobbleAt: 0
        };

        runtimeState.health = {
          ok: true,
          checkedAt: now(),
          lastError: null
        };

        runtimeState.delivery = {
          ...runtimeState.delivery,
          mode: YMBridgeMode.getCurrentMode(settings),
          activeTarget: YMBridgeMode.isStandaloneMode(settings) ? "lastfm" : runtimeState.delivery?.activeTarget,
          lastDeliveryError: null
        };

        await saveSettings();
        await saveState();
        await updateBadge();

        sendResponse({
          ok: true,
          data: buildPublicState()
        });
        return;
      }

      if (message.type === "reconnect-lastfm") {
        try {
          const validated = await YMBridgeLastfmApi.validateCredentials(settings.lastfm);

          settings.lastfm = {
            ...(settings.lastfm || {}),
            sessionKey: validated.sessionKey
          };

          runtimeState.lastfm = {
            ...runtimeState.lastfm,
            connected: true,
            authMissing: false,
            sessionCheckedAt: now(),
            lastError: null
          };

          await saveSettings();
          await saveState();
          await updateBadge();
          await pushLastfmToCompanion("reconnect-lastfm");

          sendResponse({
            ok: true,
            data: {
              sessionKey: validated.sessionKey,
              state: buildPublicState()
            }
          });
        } catch (err) {
          runtimeState.lastfm = {
            ...runtimeState.lastfm,
            connected: false,
            authMissing: !YMBridgeLastfmApi.isLastfmConfigComplete(settings?.lastfm),
            sessionCheckedAt: now(),
            lastError: String(err)
          };

          await saveState();
          await updateBadge();

          sendResponse({
            ok: false,
            error: String(err)
          });
        }
        return;
      }

      if (message.type === "get-enhanced-state") {
        await refreshYandexTabState();
        refreshBrowserDiagnostics();
        await saveState();
        await updateBadge();

        sendResponse({ ok: true, data: buildEnhancedState() });
        return;
      }

      if (message.type === "open-or-focus-yandex-music") {
        const result = await openOrFocusYandexMusic();
        sendResponse({ ok: true, action: result, data: buildEnhancedState() });
        return;
      }

      if (message.type === "focus-yandex-music-tab") {
        const result = await focusYandexMusicTab();
        sendResponse({ ok: true, action: result, data: buildEnhancedState() });
        return;
      }

      if (message.type === "reload-yandex-music-tab") {
        const result = await reloadYandexMusicTab();
        sendResponse({ ok: true, action: result, data: buildEnhancedState() });
        return;
      }

      if (message.type === "open-desktop-app-page") {
        const result = await YMBridgeDelivery.openDesktopAppPage(
          settings,
          DEFAULT_SETTINGS.desktopBridge
        );
        sendResponse({ ok: true, action: result, data: buildEnhancedState() });
        return;
      }

      sendResponse({ ok: false, error: "unknown message type" });
    } catch (err) {
      errlog("message handler failed", err);
      sendResponse({ ok: false, error: String(err) });
    }
  })();

  return true;
});

(async () => {
  await loadAll();

  if (YMBridgeMode.isDesktopBridgeMode(settings)) {
    await YMBridgeDelivery.discoverDesktopBridge(
      runtimeState,
      settings,
      DEFAULT_SETTINGS.desktopBridge,
      {
        extName: EXT_NAME,
        extVersion: EXT_VERSION,
        saveState,
        updateBadge,
        log,
        warn,
        syncCompanionLastfm: pushLastfmToCompanion
      }
    );

    try {
      await YMBridgeDelivery.sendExtensionPing(
        runtimeState,
        settings,
        DEFAULT_SETTINGS.desktopBridge,
        {
          extName: EXT_NAME,
          extVersion: EXT_VERSION,
          saveState,
          updateBadge,
          log,
          warn,
          syncCompanionLastfm: pushLastfmToCompanion
        }
      );
    } catch (e) {
      log("startup extension ping failed", String(e));
    }
  } else {
    runtimeState.activeServerUrl = null;
    runtimeState.health = {
      ok: true,
      checkedAt: now(),
      lastError: null
    };

    runtimeState.delivery = runtimeState.delivery || {};
    runtimeState.delivery.mode = YMBridgeMode.getCurrentMode(settings);
    runtimeState.delivery.activeTarget = "lastfm";
    runtimeState.delivery.lastDeliveryError = null;
  }

  await pushLastfmToCompanion("startup");

  try {
    await chrome.sidePanel.setPanelBehavior({
      openPanelOnActionClick: true
    });
  } catch (e) {
    log("setPanelBehavior failed", String(e));
  }

  await refreshYandexTabState();
  refreshBrowserDiagnostics();
  await saveState();
  await updateBadge();
  startHealthTimer();
  await pumpQueue();
})();