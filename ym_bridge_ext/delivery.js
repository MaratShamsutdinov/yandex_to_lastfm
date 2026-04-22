(function () {
    const TRACK_PATH = "track";
    const HEALTH_PATH = "health";
    const EXTENSION_PING_PATH = "extension/ping";
    const DEFAULT_DESKTOP_BASE_URL = "http://127.0.0.1:5000/";

    function now() {
        return Date.now();
    }

    function sanitizeUrl(url) {
        const trimmed = String(url || "").trim();
        if (!trimmed) return "";
        return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
    }

    async function fetchWithTimeout(url, options, timeoutMs) {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), timeoutMs);

        try {
            return await fetch(url, {
                ...options,
                signal: controller.signal
            });
        } finally {
            clearTimeout(timer);
        }
    }

    function getDesktopBridgeSettings(settings, defaults) {
        const legacy = settings || {};
        const nested = legacy.desktopBridge || {};

        return {
            serverUrl: sanitizeUrl(nested.serverUrl ?? legacy.serverUrl ?? defaults.serverUrl),
            autoDiscover: nested.autoDiscover ?? legacy.autoDiscover ?? defaults.autoDiscover,
            candidateUrls: Array.isArray(nested.candidateUrls ?? legacy.candidateUrls)
                ? (nested.candidateUrls ?? legacy.candidateUrls)
                    .map(sanitizeUrl)
                    .filter(Boolean)
                : [...(defaults.candidateUrls || [])],
            healthcheckIntervalMs:
                Number(nested.healthcheckIntervalMs ?? legacy.healthcheckIntervalMs ?? defaults.healthcheckIntervalMs),
            connectTimeoutMs:
                Number(nested.connectTimeoutMs ?? legacy.connectTimeoutMs ?? defaults.connectTimeoutMs)
        };
    }

    async function healthcheckUrl(baseUrl, settings, defaults) {
        const target = sanitizeUrl(baseUrl);
        const desktop = getDesktopBridgeSettings(settings, defaults);
        const timeoutMs = Math.max(1000, desktop.connectTimeoutMs || defaults.connectTimeoutMs);

        try {
            const healthUrl = new URL(HEALTH_PATH, target).toString();

            let resp = await fetchWithTimeout(healthUrl, { method: "GET" }, timeoutMs);
            if (resp.ok) {
                return { ok: true, url: target, method: "/health" };
            }

            resp = await fetchWithTimeout(
                target,
                {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ ping: true, source: "ym-mediabridge-extension" })
                },
                timeoutMs
            );

            if (resp.ok) {
                return { ok: true, url: target, method: "POST /" };
            }

            return { ok: false, url: target, error: `HTTP ${resp.status}` };
        } catch (err) {
            return { ok: false, url: target, error: String(err) };
        }
    }

    async function discoverDesktopBridge(runtimeState, settings, defaults, hooks) {
        const desktop = getDesktopBridgeSettings(settings, defaults);
        const candidates = [];

        if (desktop.serverUrl) {
            candidates.push(desktop.serverUrl);
        }

        if (desktop.autoDiscover) {
            for (const item of desktop.candidateUrls) {
                if (item && !candidates.includes(item)) {
                    candidates.push(item);
                }
            }
        }

        for (const url of candidates) {
            const result = await healthcheckUrl(url, settings, defaults);
            if (result.ok) {
                runtimeState.activeServerUrl = result.url;
                runtimeState.health = {
                    ok: true,
                    checkedAt: now(),
                    lastError: null
                };

                runtimeState.delivery = runtimeState.delivery || {};
                runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
                runtimeState.delivery.activeTarget = result.url;
                runtimeState.delivery.lastDeliveryError = null;
                runtimeState.delivery.lastDeliveryAt = runtimeState.delivery.lastDeliveryAt || 0;

                if (hooks?.saveState) await hooks.saveState();
                if (hooks?.updateBadge) await hooks.updateBadge();
                if (hooks?.log) hooks.log("desktop bridge discovered", result);

                try {
                    await sendExtensionPing(runtimeState, settings, defaults, hooks, result.url);
                } catch (e) {
                    if (hooks?.warn) hooks.warn("extension ping after discover failed", String(e));
                }

                return result.url;
            }
        }

        runtimeState.activeServerUrl = null;
        runtimeState.health = {
            ok: false,
            checkedAt: now(),
            lastError: "No reachable localhost endpoint"
        };

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
        runtimeState.delivery.activeTarget = null;
        runtimeState.delivery.lastDeliveryError = "No reachable localhost endpoint";

        if (hooks?.saveState) await hooks.saveState();
        if (hooks?.updateBadge) await hooks.updateBadge();

        return null;
    }

    async function ensureDesktopBridge(runtimeState, settings, defaults, hooks) {
        const current = sanitizeUrl(runtimeState.activeServerUrl);
        if (current) {
            const result = await healthcheckUrl(current, settings, defaults);
            if (result.ok) {
                runtimeState.health = {
                    ok: true,
                    checkedAt: now(),
                    lastError: null
                };

                runtimeState.delivery = runtimeState.delivery || {};
                runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
                runtimeState.delivery.activeTarget = current;
                runtimeState.delivery.lastDeliveryError = null;

                if (hooks?.saveState) await hooks.saveState();
                if (hooks?.updateBadge) await hooks.updateBadge();

                return current;
            }
        }

        return await discoverDesktopBridge(runtimeState, settings, defaults, hooks);
    }

    async function sendExtensionPing(runtimeState, settings, defaults, hooks, baseUrl = null) {
        const desktop = getDesktopBridgeSettings(settings, defaults);
        const resolvedBaseUrl =
            sanitizeUrl(baseUrl || runtimeState.activeServerUrl) ||
            (await ensureDesktopBridge(runtimeState, settings, defaults, hooks));

        if (!resolvedBaseUrl) {
            throw new Error("desktop app not reachable");
        }

        const timeoutMs = Math.max(1000, desktop.connectTimeoutMs || defaults.connectTimeoutMs);
        const pingUrl = new URL(EXTENSION_PING_PATH, resolvedBaseUrl).toString();

        const resp = await fetchWithTimeout(
            pingUrl,
            {
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    schema_version: 1,
                    client_name: hooks.extName,
                    client_version: hooks.extVersion,
                    sent_at: now(),
                    yandex_tab_open: !!runtimeState.yandexTabOpen,
                    metadata_active: !!runtimeState.metadataActive,
                    reload_likely_needed: !!runtimeState.reloadLikelyNeeded
                })
            },
            timeoutMs
        );

        const text = await resp.text();

        if (!resp.ok) {
            throw new Error(`HTTP ${resp.status}: ${text}`);
        }

        runtimeState.health = {
            ok: true,
            checkedAt: now(),
            lastError: null
        };

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
        runtimeState.delivery.activeTarget = resolvedBaseUrl;
        runtimeState.delivery.lastDeliveryError = null;

        if (hooks?.syncCompanionLastfm) {
            try {
                await hooks.syncCompanionLastfm("extension-ping");
            } catch (e) {
                if (hooks?.warn) hooks.warn("sync after extension ping failed", String(e));
            }
        }

        return true;
    }

    async function postEnvelopeToDesktopBridge(envelope, runtimeState, settings, defaults, hooks) {
        const desktop = getDesktopBridgeSettings(settings, defaults);
        const baseUrl = await ensureDesktopBridge(runtimeState, settings, defaults, hooks);

        if (!baseUrl) {
            throw new Error("desktop app not reachable");
        }

        const timeoutMs = Math.max(1000, desktop.connectTimeoutMs || defaults.connectTimeoutMs);
        const trackUrl = new URL(TRACK_PATH, baseUrl).toString();

        const resp = await fetchWithTimeout(
            trackUrl,
            {
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify(envelope)
            },
            timeoutMs
        );

        const text = await resp.text();

        if (!resp.ok) {
            throw new Error(`HTTP ${resp.status}: ${text}`);
        }

        let parsed = null;
        try {
            parsed = JSON.parse(text);
        } catch (_) { }

        runtimeState.health = {
            ok: true,
            checkedAt: now(),
            lastError: null
        };

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
        runtimeState.delivery.activeTarget = baseUrl;
        runtimeState.delivery.lastDeliveryError = null;
        runtimeState.delivery.lastDeliveryAt = now();

        return {
            status: resp.status,
            text,
            json: parsed
        };
    }

    async function ensureStandaloneSession(settings, hooks) {
        const cfg = settings?.lastfm || {};

        if (cfg.sessionKey && String(cfg.sessionKey).trim()) {
            return String(cfg.sessionKey).trim();
        }

        const result = await globalThis.YMBridgeLastfmApi.validateCredentials(cfg);

        settings.lastfm = {
            ...(settings.lastfm || {}),
            sessionKey: result.sessionKey
        };

        if (hooks?.saveSettings) {
            await hooks.saveSettings();
        }

        return result.sessionKey;
    }

    async function deliverNowPlayingStandalone(envelope, runtimeState, settings, hooks) {
        await ensureStandaloneSession(settings, hooks);

        const resp = await globalThis.YMBridgeLastfmApi.updateNowPlaying(
            settings.lastfm,
            envelope.artist,
            envelope.track,
            envelope.album || "",
            envelope.duration ?? null
        );

        runtimeState.lastfm = runtimeState.lastfm || {};
        runtimeState.lastfm.connected = true;
        runtimeState.lastfm.authMissing = false;
        runtimeState.lastfm.sessionCheckedAt = now();
        runtimeState.lastfm.lastError = null;
        runtimeState.lastfm.lastNowPlayingAt = now();

        return resp;
    }

    async function postEnvelopeToStandaloneLastfm(envelope, runtimeState, settings, hooks) {
        await ensureStandaloneSession(settings, hooks);

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
        runtimeState.delivery.activeTarget = "lastfm";

        runtimeState.lastfm = runtimeState.lastfm || {};

        if (!envelope.now_playing_sent) {
            const nowPlayingResp = await deliverNowPlayingStandalone(envelope, runtimeState, settings, hooks);
            envelope.now_playing_sent = true;

            return {
                status: 202,
                text: JSON.stringify(nowPlayingResp),
                json: nowPlayingResp,
                done: false,
                nextDelay: Math.max(
                    1000,
                    ((Number(envelope.scrobble_due_at) || Math.floor(now() / 1000) + 30) * 1000) - now()
                )
            };
        }

        const scrobbleKey = `${String(envelope.track_key || "")}|${String(envelope.started_at || 0)}`;
        if (runtimeState.dedupeMap?.[scrobbleKey]) {
            envelope.scrobble_sent = true;

            runtimeState.health = {
                ok: true,
                checkedAt: now(),
                lastError: null
            };

            runtimeState.lastfm.connected = true;
            runtimeState.lastfm.authMissing = false;
            runtimeState.lastfm.sessionCheckedAt = now();
            runtimeState.lastfm.lastError = null;

            return {
                status: 200,
                text: "duplicate scrobble skipped",
                json: { skipped: true, duplicate: true },
                done: true
            };
        }

        const dueAt = Number.isFinite(Number(envelope.scrobble_due_at))
            ? Math.floor(Number(envelope.scrobble_due_at))
            : (Math.floor(Number(envelope.started_at) || now() / 1000) + 30);

        const remainingMs = dueAt * 1000 - now();
        if (remainingMs > 0) {
            runtimeState.health = {
                ok: true,
                checkedAt: now(),
                lastError: null
            };

            runtimeState.lastfm.connected = true;
            runtimeState.lastfm.authMissing = false;
            runtimeState.lastfm.sessionCheckedAt = now();
            runtimeState.lastfm.lastError = null;

            return {
                status: 202,
                text: "scrobble deferred",
                json: { deferred: true, remainingMs },
                done: false,
                nextDelay: Math.max(1000, remainingMs)
            };
        }

        const timestamp = Number.isFinite(Number(envelope.started_at))
            ? Math.floor(Number(envelope.started_at))
            : Math.floor(now() / 1000);

        const result = await globalThis.YMBridgeLastfmApi.scrobble(
            settings.lastfm,
            envelope.artist,
            envelope.track,
            timestamp,
            envelope.album || "",
            envelope.duration ?? null
        );

        envelope.scrobble_sent = true;

        runtimeState.health = {
            ok: true,
            checkedAt: now(),
            lastError: null
        };

        runtimeState.delivery.lastDeliveryError = null;
        runtimeState.delivery.lastDeliveryAt = now();

        runtimeState.lastfm.connected = true;
        runtimeState.lastfm.authMissing = false;
        runtimeState.lastfm.sessionCheckedAt = now();
        runtimeState.lastfm.lastError = null;
        runtimeState.lastfm.lastScrobbleAt = now();

        return {
            status: 200,
            text: JSON.stringify(result),
            json: result,
            done: true
        };
    }

    async function deliverEnvelope(envelope, runtimeState, settings, defaults, hooks) {
        const mode = globalThis.YMBridgeMode.getCurrentMode(settings);

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = mode;

        if (globalThis.YMBridgeMode.isDesktopBridgeMode(settings)) {
            return await postEnvelopeToDesktopBridge(envelope, runtimeState, settings, defaults, hooks);
        }

        try {
            return await postEnvelopeToStandaloneLastfm(envelope, runtimeState, settings, hooks);
        } catch (err) {
            runtimeState.health = {
                ok: false,
                checkedAt: now(),
                lastError: String(err)
            };

            runtimeState.delivery = runtimeState.delivery || {};
            runtimeState.delivery.mode = mode;
            runtimeState.delivery.activeTarget = "lastfm";
            runtimeState.delivery.lastDeliveryError = String(err);

            runtimeState.lastfm = runtimeState.lastfm || {};
            runtimeState.lastfm.connected = false;
            runtimeState.lastfm.authMissing =
                !globalThis.YMBridgeLastfmApi.isLastfmConfigComplete(settings?.lastfm) ||
                !String(settings?.lastfm?.sessionKey || "").trim();
            runtimeState.lastfm.sessionCheckedAt = now();
            runtimeState.lastfm.lastError = String(err);

            throw err;
        }
    }

    async function retryNow(runtimeState, settings, defaults, hooks) {
        if (globalThis.YMBridgeMode.isDesktopBridgeMode(settings)) {
            await discoverDesktopBridge(runtimeState, settings, defaults, hooks);

            try {
                await sendExtensionPing(runtimeState, settings, defaults, hooks);
            } catch (e) {
                if (hooks?.log) hooks.log("extension ping after retry-now failed", String(e));
            }

            return;
        }

        await ensureStandaloneSession(settings, hooks);

        runtimeState.delivery = runtimeState.delivery || {};
        runtimeState.delivery.mode = globalThis.YMBridgeMode.getCurrentMode(settings);
        runtimeState.delivery.activeTarget = "lastfm";

        runtimeState.lastfm = runtimeState.lastfm || {};
        runtimeState.lastfm.connected = true;
        runtimeState.lastfm.authMissing = false;
        runtimeState.lastfm.sessionCheckedAt = now();
        runtimeState.lastfm.lastError = null;
    }

    async function openDesktopAppPage(settings, defaults) {
        const desktop = getDesktopBridgeSettings(settings, defaults);
        const target = sanitizeUrl(desktop.serverUrl) || DEFAULT_DESKTOP_BASE_URL;

        await chrome.tabs.create({ url: target, active: true });

        return {
            ok: true,
            url: target
        };
    }

    globalThis.YMBridgeDelivery = {
        sanitizeUrl,
        fetchWithTimeout,
        getDesktopBridgeSettings,
        healthcheckUrl,
        discoverDesktopBridge,
        ensureDesktopBridge,
        sendExtensionPing,
        ensureStandaloneSession,
        deliverNowPlayingStandalone,
        postEnvelopeToDesktopBridge,
        postEnvelopeToStandaloneLastfm,
        deliverEnvelope,
        retryNow,
        openDesktopAppPage
    };
})();