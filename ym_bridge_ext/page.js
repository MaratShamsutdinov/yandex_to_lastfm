(() => {
    const TAG = "[YM-BRIDGE page]";
    const SOURCE = "ym-bridge-page";
    const FLUSH_DELAY_MS = 250;
    const RECONCILE_INTERVAL_MS = 4000;

    if (window.__ymBridgePageInstalled) {
        console.log(TAG, "already installed");
        return;
    }
    window.__ymBridgePageInstalled = true;

    console.log(TAG, "init", location.href);

    let lastEmittedKey = "";
    let lastDuration = null;

    let pendingMeta = null;
    let pendingReason = "";
    let pendingTimer = null;
    let pendingSeq = 0;

    function makeEventId() {
        return `${Date.now()}-${Math.random().toString(16).slice(2, 10)}`;
    }

    function send(kind, payload) {
        const msg = {
            source: SOURCE,
            kind,
            ts: Date.now(),
            event_id: makeEventId(),
            ...payload
        };

        console.log(TAG, "send", msg);
        window.postMessage(msg, "*");
    }

    function sendHeartbeat(info) {
        const msg = {
            source: SOURCE,
            kind: "metadata-heartbeat",
            ts: Date.now(),
            event_id: makeEventId(),
            artist: info.artist,
            track: info.track,
            album: info.album,
            cover_url: info.cover_url,
            duration: Number.isFinite(lastDuration) ? lastDuration : null
        };

        console.log(TAG, "heartbeat", msg);
        window.postMessage(msg, "*");
    }

    function cleanText(value, maxLen = 300) {
        return String(value || "").trim().slice(0, maxLen);
    }

    function normalizeMetadata(meta) {
        if (!meta) {
            return {
                artist: "",
                track: "",
                album: "",
                cover_url: ""
            };
        }

        const artist = cleanText(meta.artist);
        const track = cleanText(meta.title);
        const album = cleanText(meta.album);

        let cover_url = "";
        try {
            if (Array.isArray(meta.artwork) && meta.artwork.length > 0) {
                const best = meta.artwork[meta.artwork.length - 1];
                cover_url = cleanText(best?.src || "", 1000);
            }
        } catch (err) {
            console.warn(TAG, "artwork parse failed", err);
        }

        if (cover_url && !/^https?:\/\//i.test(cover_url)) {
            cover_url = "";
        }

        return {
            artist,
            track,
            album,
            cover_url
        };
    }

    function metadataKey(info) {
        return `${info.artist} | ${info.track} | ${info.album} | ${info.cover_url}`;
    }

    function flushPendingMetadata(seq) {
        if (seq !== pendingSeq) return;
        if (!pendingMeta) return;

        const info = normalizeMetadata(pendingMeta);
        const key = metadataKey(info);

        if (!info.artist || !info.track) {
            console.log(TAG, "skip incomplete metadata", info);
            return;
        }

        if (key === lastEmittedKey) {
            console.log(TAG, "skip duplicate metadata", key);
            return;
        }

        lastEmittedKey = key;

        send("metadata", {
            reason: pendingReason,
            artist: info.artist,
            track: info.track,
            album: info.album,
            cover_url: info.cover_url,
            duration: Number.isFinite(lastDuration) ? lastDuration : null
        });
    }

    function scheduleMetadata(meta, reason) {
        pendingMeta = meta;
        pendingReason = reason;
        pendingSeq += 1;

        const seq = pendingSeq;

        if (pendingTimer) {
            clearTimeout(pendingTimer);
            pendingTimer = null;
        }

        pendingTimer = setTimeout(() => {
            pendingTimer = null;
            flushPendingMetadata(seq);
        }, FLUSH_DELAY_MS);
    }

    function hookMetadataProperty() {
        const mediaSession = navigator.mediaSession;
        if (!mediaSession) {
            console.log(TAG, "navigator.mediaSession not found");
            return false;
        }

        const proto = Object.getPrototypeOf(mediaSession);
        if (!proto) {
            console.log(TAG, "mediaSession prototype not found");
            return false;
        }

        const desc = Object.getOwnPropertyDescriptor(proto, "metadata");
        if (!desc || !desc.configurable || !desc.get || !desc.set) {
            console.log(TAG, "metadata descriptor unsuitable", desc);
            return false;
        }

        const originalGet = desc.get;
        const originalSet = desc.set;

        Object.defineProperty(proto, "metadata", {
            configurable: true,
            enumerable: desc.enumerable,
            get() {
                return originalGet.call(this);
            },
            set(value) {
                originalSet.call(this, value);

                try {
                    scheduleMetadata(value, "setter");
                } catch (err) {
                    console.error(TAG, "schedule metadata failed", err);
                }
            }
        });

        console.log(TAG, "metadata hook installed");

        try {
            const current = originalGet.call(mediaSession);
            if (current) {
                scheduleMetadata(current, "initial-read");
            }
        } catch (err) {
            console.error(TAG, "initial metadata read failed", err);
        }

        return true;
    }

    function hookPositionState() {
        const mediaSession = navigator.mediaSession;
        if (!mediaSession || typeof mediaSession.setPositionState !== "function") {
            console.log(TAG, "setPositionState not found");
            return false;
        }

        const original = mediaSession.setPositionState.bind(mediaSession);

        mediaSession.setPositionState = function (state) {
            try {
                if (state && typeof state.duration === "number" && Number.isFinite(state.duration)) {
                    if (state.duration > 0 && state.duration < 24 * 60 * 60) {
                        lastDuration = state.duration;
                        console.log(TAG, "duration updated", lastDuration);
                    }
                }
            } catch (err) {
                console.error(TAG, "position hook failed", err);
            }

            return original(state);
        };

        console.log(TAG, "position hook installed");
        return true;
    }

    function startReconcileLoop() {
        setInterval(() => {
            try {
                const current = navigator.mediaSession?.metadata;
                if (!current) return;

                const info = normalizeMetadata(current);
                const key = metadataKey(info);

                if (!info.artist || !info.track) return;

                if (key === lastEmittedKey) {
                    sendHeartbeat(info);
                    return;
                }

                console.log(TAG, "reconcile metadata", info);
                scheduleMetadata(current, "reconcile");
            } catch (err) {
                console.error(TAG, "reconcile failed", err);
            }
        }, RECONCILE_INTERVAL_MS);
    }

    function boot() {
        const metadataHook = hookMetadataProperty();
        const positionHook = hookPositionState();

        console.log(TAG, "boot", {
            metadata_hook: metadataHook,
            position_hook: positionHook,
            href: location.href
        });

        startReconcileLoop();
    }

    boot();
})();