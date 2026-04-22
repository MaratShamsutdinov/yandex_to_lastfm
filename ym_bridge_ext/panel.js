function fmtTime(ts) {
    if (!ts) return "—";
    try {
        return new Date(ts).toLocaleString();
    } catch {
        return String(ts);
    }
}

function fmtRelativeTime(ts) {
    if (!ts) return "—";

    const diffMs = Date.now() - Number(ts);
    if (!Number.isFinite(diffMs)) return "—";

    const diffSec = Math.max(0, Math.floor(diffMs / 1000));

    // до минуты — не дёргаем UI
    if (diffSec < 60) return "just now";

    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) {
        return `${diffMin} min${diffMin === 1 ? "" : "s"} ago`;
    }

    const diffHours = Math.floor(diffMin / 60);
    if (diffHours < 24) {
        return `${diffHours} hr${diffHours === 1 ? "" : "s"} ago`;
    }

    const diffDays = Math.floor(diffHours / 24);
    return `${diffDays} day${diffDays === 1 ? "" : "s"} ago`;
}

function $(id) {
    return document.getElementById(id);
}

function setText(id, value) {
    const el = $(id);
    if (el) {
        el.textContent = value ?? "—";
    }
}

function setTrackColor(rgb) {
    const host = $("trackCardHost");
    const cover = $("coverBox");

    const normalized = Array.isArray(rgb) && rgb.length === 3
        ? [rgb[0], rgb[1], rgb[2]]
        : null;

    const same =
        Array.isArray(currentTrackRgb) &&
        Array.isArray(normalized) &&
        currentTrackRgb[0] === normalized[0] &&
        currentTrackRgb[1] === normalized[1] &&
        currentTrackRgb[2] === normalized[2];

    if (same) return;

    currentTrackRgb = normalized;

    const value = normalized
        ? `${normalized[0]}, ${normalized[1]}, ${normalized[2]}`
        : "255, 255, 255";

    host?.style.setProperty("--track-rgb", value);
    cover?.style.setProperty("--track-rgb", value);
}

function clampByte(n) {
    return Math.max(0, Math.min(255, Math.round(Number(n) || 0)));
}

function normalizeTrackRgb(r, g, b) {
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);

    let nr = r;
    let ng = g;
    let nb = b;

    // слегка поднимаем слишком тёмные цвета
    if (max < 70) {
        const lift = 70 - max;
        nr += lift;
        ng += lift;
        nb += lift;
    }

    // убираем грязно-серые совсем тусклые оттенки
    if ((max - min) < 18) {
        nr = nr * 0.85 + 18;
        ng = ng * 0.85 + 18;
        nb = nb * 0.90 + 26;
    }

    return [
        clampByte(nr),
        clampByte(ng),
        clampByte(nb)
    ];
}

function sampleDominantColorFromImage(img) {
    try {
        const size = 24;
        const canvas = document.createElement("canvas");
        canvas.width = size;
        canvas.height = size;

        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (!ctx) return null;

        ctx.drawImage(img, 0, 0, size, size);

        const { data } = ctx.getImageData(0, 0, size, size);

        let rSum = 0;
        let gSum = 0;
        let bSum = 0;
        let count = 0;

        for (let i = 0; i < data.length; i += 4) {
            const a = data[i + 3];
            if (a < 180) continue;

            const r = data[i];
            const g = data[i + 1];
            const b = data[i + 2];

            // отсекаем почти чёрные и почти белые пиксели
            const max = Math.max(r, g, b);
            const min = Math.min(r, g, b);
            const sat = max - min;

            if (max < 28) continue;
            if (min > 242) continue;

            // чуть больше веса насыщенным цветам
            const weight = 1 + sat / 64;

            rSum += r * weight;
            gSum += g * weight;
            bSum += b * weight;
            count += weight;
        }

        if (!count) return null;

        return normalizeTrackRgb(rSum / count, gSum / count, bSum / count);
    } catch {
        return null;
    }
}

function setChip(id, text, tone = "") {
    const el = $(id);
    if (!el) return;

    el.textContent = text;
    el.className = `chip${tone ? ` ${tone}` : ""}`;
}

async function sendMessage(type, extra = {}) {
    return await chrome.runtime.sendMessage({ type, ...extra });
}

let liveRefreshTimer = null;
let refreshInFlight = false;
let currentCoverUrl = "";
let currentTrackRgb = null;

async function getState() {
    const resp = await sendMessage("get-enhanced-state");

    if (resp?.ok) return resp;
    return await sendMessage("get-state");
}

function resolveStatus(data) {
    const runtime = data.runtime || {};
    const browser = runtime.browser || {};
    const ui = data.ui || null;
    const mode = data.settings?.mode || "desktop_bridge";
    const hasRuntimeAuth = !!globalThis.YMBridgeLastfmApi?.hasStandaloneRuntimeAuth?.(data.settings?.lastfm);
    const lastfmConnected = !!runtime.lastfm?.connected;
    const lastfmError = String(runtime.lastfm?.lastError || "").trim();
    const standalonePending = Array.isArray(runtime.queue)
        ? runtime.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
        : false;

    if (ui?.status && ui?.hint) {
        return {
            summary: mapUiStatusToLabel(ui.status),
            hint: ui.hint
        };
    }

    if (mode === "standalone") {
        if (!hasRuntimeAuth) {
            return {
                summary: "Last.fm connection needed",
                hint: "Open settings and connect Last.fm."
            };
        }

        if (lastfmError) {
            return {
                summary: "Last.fm error",
                hint: lastfmError
            };
        }

        if (!browser.yandexTabOpen) {
            return {
                summary: "Open Yandex Music",
                hint: "Open music.yandex.ru to start tracking."
            };
        }

        if (browser.reloadLikelyNeeded) {
            return {
                summary: "Reload Yandex Music tab",
                hint: "The tab is open, but metadata is not flowing yet."
            };
        }

        if (standalonePending) {
            return {
                summary: "Scrobble pending",
                hint: "Now Playing sent. Waiting before the scrobble."
            };
        }

        if (browser.metadataActive && lastfmConnected) {
            return {
                summary: "Connected",
                hint: "Yandex Music and Last.fm are both active."
            };
        }

        if (browser.metadataActive) {
            return {
                summary: "Metadata active",
                hint: "Track metadata is flowing."
            };
        }

        return {
            summary: "Waiting for track metadata",
            hint: "Waiting for Yandex Music metadata."
        };
    }

    if (!runtime.health?.ok) {
        return {
            summary: "Desktop companion unavailable",
            hint: "Start the desktop companion or switch to standalone mode."
        };
    }

    if (browser.reloadLikelyNeeded) {
        return {
            summary: "Reload Yandex Music tab",
            hint: "The tab is open, but metadata is not flowing yet."
        };
    }

    if (!browser.yandexTabOpen) {
        return {
            summary: "Open Yandex Music",
            hint: "Open music.yandex.ru to start detection."
        };
    }

    if (browser.metadataActive) {
        return {
            summary: "Connected",
            hint: "Track metadata is flowing."
        };
    }

    return {
        summary: "Waiting for track metadata",
        hint: "Waiting for Yandex Music metadata."
    };
}



function mapUiStatusToLabel(status) {
    switch (status) {
        case "connected":
            return "Connected";
        case "desktop-app-not-detected":
            return "Desktop companion unavailable";
        case "open-yandex-music":
            return "Open Yandex Music";
        case "reload-yandex-music-tab":
            return "Reload Yandex Music tab";
        case "waiting-for-track-metadata":
            return "Waiting for track metadata";
        case "lastfm-auth-missing":
            return "Last.fm connection needed";
        case "lastfm-error":
            return "Last.fm error";
        case "scrobble-pending":
            return "Scrobble pending";
        default:
            return "YM Bridge";
    }
}

function shouldShowCompanionHint(data) {
    const mode = data.settings?.mode || "desktop_bridge";
    const companionAlive = !!data.runtime?.health?.ok;
    const hasErrors = !!String(data.runtime?.lastfm?.lastError || "").trim();
    const hasQueue = Number(data.runtime?.queueLength || 0) > 0;

    return (
        mode === "standalone" &&
        !companionAlive &&
        (hasErrors || hasQueue)
    );
}

function renderCompanionHint(data) {
    const el = $("companionHint");
    if (!el) return;

    el.classList.toggle("hidden", !shouldShowCompanionHint(data));
}

function rebalanceSecondaryGrid() {
    const host = $("secondaryActions");
    if (!host) return;

    const buttons = Array.from(host.querySelectorAll("button"));

    for (const btn of buttons) {
        btn.classList.remove("wide");
    }

    const visibleButtons = buttons.filter(btn => !btn.classList.contains("hidden"));

    if (visibleButtons.length % 2 === 1) {
        visibleButtons[visibleButtons.length - 1].classList.add("wide");
    }
}

function renderCover(lastTrack) {
    const box = $("coverBox");
    if (!box) return;

    const coverUrl = String(lastTrack?.cover_url || "").trim();

    if (!coverUrl) {
        currentCoverUrl = "";
        setTrackColor(null);
        box.classList.remove("hasImage");
        box.innerHTML = `<span class="muted">No cover</span>`;
        return;
    }

    // если обложка не менялась — вообще не трогаем DOM и glow
    if (coverUrl === currentCoverUrl && box.querySelector("img")) {
        return;
    }

    currentCoverUrl = coverUrl;

    const img = document.createElement("img");
    img.src = coverUrl;
    img.alt = "Cover";
    img.referrerPolicy = "no-referrer";
    img.crossOrigin = "anonymous";

    img.onerror = () => {
        if (coverUrl !== currentCoverUrl) return;
        setTrackColor(null);
        box.classList.remove("hasImage");
        box.innerHTML = `<span class="muted">No cover</span>`;
    };

    img.onload = () => {
        if (coverUrl !== currentCoverUrl) return;
        const rgb = sampleDominantColorFromImage(img);
        setTrackColor(rgb);
        box.classList.add("hasImage");
    };

    box.classList.remove("hasImage");
    box.innerHTML = "";
    box.appendChild(img);
}

function renderMetaInfo(runtime, settings) {
    const mode = settings?.mode || "desktop_bridge";
    const desktop = settings?.desktopBridge || {};
    const delivery = runtime?.delivery || {};
    const lastfm = runtime?.lastfm || {};
    const effectiveError = String(lastfm.lastError || runtime.lastError || "").trim();

    setText(
        "transportLabel",
        mode === "standalone" ? "Delivery target" : "Desktop target"
    );

    setText(
        "serverUrl",
        mode === "standalone"
            ? (delivery.activeTarget === "lastfm" ? "direct to Last.fm" : (delivery.activeTarget || "direct to Last.fm"))
            : (runtime.activeServerUrl || desktop.serverUrl || "—")
    );

    setText(
        "lastSuccess",
        fmtRelativeTime(
            mode === "standalone"
                ? (lastfm.lastScrobbleAt || lastfm.lastNowPlayingAt || runtime.lastSuccessAt)
                : runtime.lastSuccessAt
        )
    );

    setText("lastError", effectiveError || "—");

    const lastErrorLabel = $("lastErrorLabel");
    const lastErrorValue = $("lastError");

    if (effectiveError) {
        lastErrorLabel?.classList.remove("hidden");
        lastErrorValue?.classList.remove("hidden");
    } else {
        lastErrorLabel?.classList.add("hidden");
        lastErrorValue?.classList.add("hidden");
    }
}

function renderButtons(data) {
    const runtime = data.runtime || {};
    const settings = data.settings || {};
    const browser = runtime.browser || {};
    const mode = settings.mode || "desktop_bridge";
    const hasRuntimeAuth = !!globalThis.YMBridgeLastfmApi?.hasStandaloneRuntimeAuth?.(settings?.lastfm);
    const queueLength = Number(runtime.queueLength || 0);
    const standalonePending = Array.isArray(runtime.queue)
        ? runtime.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
        : false;

    const focusBtn = $("focusYmBtn");
    const reloadBtn = $("reloadYmBtn");
    const retryBtn = $("retryBtn");
    const optionsBtn = $("optionsBtn");
    const appPageBtn = $("appPageBtn");
    const copyBtn = $("copyBtn");

    const canRetry =
        queueLength > 0 ||
        standalonePending ||
        !!runtime.lastfm?.lastError;

    if (focusBtn) {
        focusBtn.disabled = !browser.yandexTabOpen;
        focusBtn.textContent = "Focus tab";
    }

    if (reloadBtn) {
        reloadBtn.disabled = !browser.yandexTabOpen;
        reloadBtn.textContent = "Reload tab";
    }

    if (retryBtn) {
        retryBtn.textContent = mode === "standalone" ? "Retry delivery" : "Retry queue";
        retryBtn.disabled = !canRetry || (mode === "standalone" && !hasRuntimeAuth);
        retryBtn.classList.toggle("hidden", !canRetry);
    }

    if (optionsBtn) {
        optionsBtn.textContent = mode === "standalone"
            ? "Last.fm settings"
            : "Extension settings";
    }

    if (appPageBtn) {
        const showAppBtn = mode === "desktop_bridge";
        appPageBtn.classList.toggle("hidden", !showAppBtn);
        appPageBtn.disabled = !showAppBtn;
        appPageBtn.textContent = "Desktop companion";
    }

    if (copyBtn) {
        copyBtn.textContent = "Copy diagnostics";
    }

    rebalanceSecondaryGrid();
}

function resolvePrimaryAction(data) {
    const runtime = data.runtime || {};
    const browser = runtime.browser || {};
    const mode = data.settings?.mode || "desktop_bridge";
    const hasRuntimeAuth = !!globalThis.YMBridgeLastfmApi?.hasStandaloneRuntimeAuth?.(data.settings?.lastfm);
    const lastfmError = String(runtime.lastfm?.lastError || "").trim();
    const standalonePending = Array.isArray(runtime.queue)
        ? runtime.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
        : false;
    const queueLength = Number(runtime.queueLength || 0);

    if (mode === "standalone") {
        if (!hasRuntimeAuth) {
            return {
                label: "Connect Last.fm",
                actionType: "open-options-page",
                tone: "danger"
            };
        }

        if (lastfmError) {
            return {
                label: "Reconnect Last.fm",
                actionType: "open-options-page",
                tone: "danger"
            };
        }

        if (!browser.yandexTabOpen) {
            return {
                label: "Open Yandex Music",
                actionType: "open-or-focus-yandex-music",
                tone: "accent"
            };
        }

        if (browser.reloadLikelyNeeded || !browser.metadataHookEstablished) {
            return {
                label: "Reload Yandex Music tab",
                actionType: "reload-yandex-music-tab",
                tone: "accent"
            };
        }

        if (standalonePending || queueLength > 0) {
            return {
                label: "Retry queue",
                actionType: "retry-now",
                tone: "neutral"
            };
        }

        return {
            label: "Focus Yandex Music",
            actionType: "focus-yandex-music-tab",
            tone: "neutral"
        };
    }

    if (!runtime.health?.ok) {
        return {
            label: "Open desktop companion",
            actionType: "open-desktop-app-page",
            tone: "danger"
        };
    }

    if (!browser.yandexTabOpen) {
        return {
            label: "Open Yandex Music",
            actionType: "open-or-focus-yandex-music",
            tone: "accent"
        };
    }

    if (browser.reloadLikelyNeeded || !browser.metadataHookEstablished) {
        return {
            label: "Reload Yandex Music tab",
            actionType: "reload-yandex-music-tab",
            tone: "accent"
        };
    }

    if (queueLength > 0) {
        return {
            label: "Retry queue",
            actionType: "retry-now",
            tone: "neutral"
        };
    }

    return {
        label: "Focus Yandex Music",
        actionType: "focus-yandex-music-tab",
        tone: "neutral"
    };
}


function renderPrimaryAction(data) {
    const btn = $("primaryActionBtn");
    const focusBtn = $("focusYmBtn");
    const reloadBtn = $("reloadYmBtn");

    if (!btn) return;

    const action = resolvePrimaryAction(data);

    btn.textContent = action.label || "—";
    btn.dataset.actionType = action.actionType || "";

    const tone = action.tone || "neutral";
    const shouldPulse =
        action.actionType === "open-options-page" ||
        action.actionType === "reload-yandex-music-tab";

    btn.className = `primaryActionBtn ready ${tone}${shouldPulse ? " pulse" : ""}`;

    // убираем визуальный дубль главного действия из secondary ряда
    if (focusBtn) {
        focusBtn.classList.toggle("hidden", action.actionType === "focus-yandex-music-tab");
    }

    if (reloadBtn) {
        reloadBtn.classList.toggle("hidden", action.actionType === "reload-yandex-music-tab");
    }
}

function renderHeroTone(data) {
    const hero = $("heroCard");
    if (!hero) return;

    const runtime = data.runtime || {};
    const browser = runtime.browser || {};
    const mode = data.settings?.mode || "desktop_bridge";
    const hasSessionKey = !!String(data.settings?.lastfm?.sessionKey || "").trim();
    const lastfmError = String(runtime.lastfm?.lastError || "").trim();
    const standalonePending = Array.isArray(runtime.queue)
        ? runtime.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
        : false;

    let tone = "";

    if (mode === "standalone") {
        if (!hasSessionKey || lastfmError) {
            tone = "danger";
        } else if (standalonePending) {
            tone = "accent";
        } else if (browser.metadataActive && runtime.lastfm?.connected) {
            tone = "ok";
        } else if (browser.reloadLikelyNeeded || !browser.metadataHookEstablished) {
            tone = "accent";
        }
    } else {
        if (!runtime.health?.ok) {
            tone = "danger";
        } else if (browser.metadataActive) {
            tone = "ok";
        } else if (browser.reloadLikelyNeeded || !browser.metadataHookEstablished) {
            tone = "accent";
        }
    }

    hero.className = `card heroCard${tone ? ` ${tone}` : ""}`;
}

function render(data) {
    const runtime = data.runtime || {};
    const settings = data.settings || {};
    const browser = runtime.browser || {};
    const status = resolveStatus(data);
    const mode = settings.mode || "desktop_bridge";
    const queueLength = Number(runtime.queueLength || 0);
    const standalonePending = Array.isArray(runtime.queue)
        ? runtime.queue.some(item => item.now_playing_sent && !item.scrobble_sent)
        : false;

    setText("statusSummary", status.summary);
    setText("statusHint", status.hint);

    setChip(
        "chipDesktop",
        mode === "standalone"
            ? `Last.fm: ${runtime.lastfm?.connected ? "connected" : "needs attention"}`
            : `Desktop companion: ${runtime.health?.ok ? "connected" : "unavailable"}`,
        mode === "standalone"
            ? (runtime.lastfm?.lastError ? "bad" : (runtime.lastfm?.connected ? "ok" : "warn"))
            : (runtime.health?.ok ? "ok" : "bad")
    );

    setChip(
        "chipTab",
        `Yandex Music: ${browser.yandexTabOpen ? `open (${browser.yandexTabCount || 1})` : "closed"}`,
        browser.yandexTabOpen ? "ok" : "warn"
    );

    setChip(
        "chipMeta",
        `Track feed: ${browser.metadataActive ? "active" : "waiting"}`,
        browser.metadataActive ? "ok" : (browser.reloadLikelyNeeded ? "warn" : "warn")
    );

    setChip(
        "chipQueue",
        standalonePending
            ? "Queue: scrobble pending"
            : `Queue: ${queueLength}`,
        standalonePending ? "warn" : (queueLength > 0 ? "warn" : "ok")
    );

    const lastTrack = runtime.lastTrack || null;

    setText("artistText", lastTrack?.artist || "No track yet");
    setText("trackText", lastTrack?.track || "—");
    setText("albumText", lastTrack?.album || "—");

    renderMetaInfo(runtime, settings);
    setText("diag", JSON.stringify(data, null, 2));

    renderCover(lastTrack);
    renderPrimaryAction(data);
    renderButtons(data);
    renderCompanionHint(data);
    renderHeroTone(data);
}

async function refresh() {
    if (refreshInFlight) return;
    refreshInFlight = true;

    try {
        const resp = await getState();
        if (!resp?.ok) {
            setText("statusSummary", "Error");
            setText("statusHint", resp?.error || "Failed to load state");
            setText("diag", JSON.stringify(resp, null, 2));
            return;
        }

        try {
            render(resp.data);
        } catch (err) {
            console.error("[YM-BRIDGE panel] render failed", err);
            setText("statusSummary", "Panel render error");
            setText("statusHint", String(err));
            setText("diag", String(err) + "\n\n" + (err?.stack || ""));
        }
    } catch (err) {
        console.error("[YM-BRIDGE panel] refresh failed", err);
        setText("statusSummary", "Panel refresh error");
        setText("statusHint", String(err));
        setText("diag", String(err) + "\n\n" + (err?.stack || ""));
    } finally {
        refreshInFlight = false;
    }
}

function startLiveRefresh() {
    stopLiveRefresh();

    liveRefreshTimer = setInterval(() => {
        refresh();
    }, 1000);
}

function stopLiveRefresh() {
    if (liveRefreshTimer) {
        clearInterval(liveRefreshTimer);
        liveRefreshTimer = null;
    }
}

$("primaryActionBtn").addEventListener("click", async () => {
    const btn = $("primaryActionBtn");
    const type = btn?.dataset?.actionType;

    if (!type) return;

    if (type === "open-options-page") {
        chrome.runtime.openOptionsPage();
        return;
    }

    const resp = await sendMessage(type);
    if (resp?.ok && resp.data) {
        render(resp.data);
    } else {
        await refresh();
    }
});

$("focusYmBtn").addEventListener("click", async () => {
    const resp = await sendMessage("focus-yandex-music-tab");
    if (resp?.ok) render(resp.data);
});

$("reloadYmBtn").addEventListener("click", async () => {
    const resp = await sendMessage("reload-yandex-music-tab");
    if (resp?.ok && resp.data) {
        render(resp.data);
    } else {
        await refresh();
    }
});

$("retryBtn").addEventListener("click", async () => {
    const resp = await sendMessage("retry-now");
    if (resp?.ok && resp.data) {
        render(resp.data);
    } else {
        await refresh();
    }
});

$("optionsBtn").addEventListener("click", () => {
    chrome.runtime.openOptionsPage();
});

$("appPageBtn").addEventListener("click", async () => {
    const resp = await sendMessage("open-desktop-app-page");
    if (resp?.ok && resp.data) {
        render(resp.data);
    } else {
        await refresh();
    }
});

$("copyBtn").addEventListener("click", async () => {
    const resp = await getState();
    if (resp?.ok) {
        await navigator.clipboard.writeText(JSON.stringify(resp.data, null, 2));
    }
});

$("installCompanionBtn")?.addEventListener("click", () => {
    chrome.tabs.create({
        url: "https://github.com/Maratej/yandex_to_lastfm"
    });
});

document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
        stopLiveRefresh();
        return;
    }

    refresh();
    startLiveRefresh();
});

window.addEventListener("DOMContentLoaded", async () => {
    try {
        await refresh();
        startLiveRefresh();
    } catch (err) {
        console.error("[YM-BRIDGE panel] startup failed", err);
        setText("statusSummary", "Panel startup error");
        setText("statusHint", String(err));
        setText("diag", String(err) + "\n\n" + (err?.stack || ""));
    }
});

window.addEventListener("unload", stopLiveRefresh);