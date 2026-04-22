(() => {
    const TAG = "[YM-BRIDGE content]";
    const PAGE_SOURCE = "ym-bridge-page";

    if (window.__ymBridgeContentInstalled) {
        console.log(TAG, "already installed");
        return;
    }
    window.__ymBridgeContentInstalled = true;

    console.log(TAG, "init", location.href);

    function isRuntimeAvailable() {
        try {
            return !!(chrome && chrome.runtime && chrome.runtime.id);
        } catch {
            return false;
        }
    }

    function injectPageScript() {
        let src = "";

        try {
            if (!chrome?.runtime?.id) {
                console.warn(TAG, "runtime unavailable before inject");
                return;
            }

            src = chrome.runtime.getURL("page.js");
        } catch (err) {
            console.warn(TAG, "getURL failed, stale extension context:", err);
            return;
        }

        const root = document.documentElement || document.head || document.body;
        if (!root) {
            console.warn(TAG, "no root element for page.js inject");
            return;
        }

        const s = document.createElement("script");
        s.src = src;
        s.async = false;

        s.onload = () => {
            console.log(TAG, "page.js loaded");
            s.remove();
        };

        s.onerror = () => {
            console.warn(TAG, "page.js inject skipped (likely stale context after reload)");
            s.remove();
        };

        try {
            root.appendChild(s);
        } catch (err) {
            const msg = String(err);
            if (msg.includes("Extension context invalidated")) {
                console.warn(TAG, "appendChild aborted: stale extension context");
                return;
            }
            console.error(TAG, "appendChild failed", err);
        }
    }

    window.addEventListener("message", (event) => {
        if (event.source !== window) return;

        const data = event.data;
        if (!data || data.source !== PAGE_SOURCE) return;
        if (data.kind !== "metadata" && data.kind !== "metadata-heartbeat") return;

        const payload = {
            ...data,
            page_url: window.location.href
        };

        console.log(TAG, "message from page", payload);

        if (!isRuntimeAvailable()) {
            console.warn(TAG, "runtime unavailable, likely after extension reload");
            return;
        }

        try {
            chrome.runtime.sendMessage(
                {
                    type: data.kind === "metadata-heartbeat"
                        ? "metadata-heartbeat"
                        : "post-metadata",
                    payload
                },
                (response) => {
                    let msg = "";

                    try {
                        if (chrome.runtime && chrome.runtime.lastError) {
                            msg = String(chrome.runtime.lastError.message || chrome.runtime.lastError);
                        }
                    } catch (_) { }

                    if (msg) {
                        if (
                            msg.includes("Extension context invalidated") ||
                            msg.includes("message port closed") ||
                            msg.includes("Receiving end does not exist")
                        ) {
                            console.warn(TAG, "sendMessage skipped:", msg);
                            return;
                        }

                        console.error(TAG, "sendMessage ERROR", msg);
                        return;
                    }

                    console.log(TAG, "bg response", response);
                }
            );
        } catch (err) {
            const msg = String(err);

            if (
                msg.includes("Extension context invalidated") ||
                msg.includes("Receiving end does not exist")
            ) {
                console.warn(TAG, "sendMessage aborted after extension reload");
                return;
            }

            console.error(TAG, "sendMessage threw", err);
        }
    });

    injectPageScript();
})();