(function () {
    const MODE_STANDALONE = "standalone";
    const MODE_DESKTOP_BRIDGE = "desktop_bridge";

    function normalizeMode(value) {
        const raw = String(value || "").trim().toLowerCase();

        if (raw === MODE_STANDALONE) return MODE_STANDALONE;
        if (raw === MODE_DESKTOP_BRIDGE) return MODE_DESKTOP_BRIDGE;

        return MODE_STANDALONE;
    }

    function getCurrentMode(settings) {
        return normalizeMode(settings?.mode);
    }

    function isStandaloneMode(settings) {
        return getCurrentMode(settings) === MODE_STANDALONE;
    }

    function isDesktopBridgeMode(settings) {
        return getCurrentMode(settings) === MODE_DESKTOP_BRIDGE;
    }

    globalThis.YMBridgeMode = {
        MODE_STANDALONE,
        MODE_DESKTOP_BRIDGE,
        normalizeMode,
        getCurrentMode,
        isStandaloneMode,
        isDesktopBridgeMode
    };
})();