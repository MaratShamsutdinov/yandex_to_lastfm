function $(id) {
    return document.getElementById(id);
}

async function getState() {
    return await chrome.runtime.sendMessage({ type: "get-state" });
}

function setStatus(text, isError = false) {
    const el = $("status");
    el.textContent = text;
    el.style.color = isError ? "#ff7b72" : "#7ee787";
}

function setLastfmValidateStatus(text, isError = false) {
    const el = $("lastfmValidateStatus");
    if (!el) return;
    el.textContent = text;
    el.style.color = isError ? "#ff7b72" : "#7ee787";
}

function setSessionPill(hasSessionKey, lastfmError = "") {
    const pill = $("lastfmSessionPill");
    if (!pill) return;

    pill.className = "pill";

    if (lastfmError) {
        pill.textContent = "Session: error";
        pill.classList.add("bad");
        return;
    }

    if (hasSessionKey) {
        pill.textContent = "Session: connected";
        pill.classList.add("ok");
        return;
    }

    pill.textContent = "Session: missing";
    pill.classList.add("warn");
}

function updateModeVisibility() {
    const isStandalone = $("modeStandalone").checked;
    $("desktopBridgeCard").classList.toggle("hidden", isStandalone);
}

function setCompanionImportHintVisible(visible) {
    const el = $("companionImportHint");
    if (!el) return;
    el.classList.toggle("hidden", !visible);
}

function hasAnyLastfmData(lastfm = {}) {
    return !!(
        String(lastfm.apiKey || "").trim() ||
        String(lastfm.apiSecret || "").trim() ||
        String(lastfm.username || "").trim() ||
        String(lastfm.password || "").trim() ||
        String(lastfm.sessionKey || "").trim()
    );
}

function loadForm(settings, runtime = null) {
    const mode = settings?.mode || "desktop_bridge";
    $("modeStandalone").checked = mode === "standalone";
    $("modeDesktopBridge").checked = mode === "desktop_bridge";

    const desktop = settings?.desktopBridge || {};
    const lastfm = settings?.lastfm || {};

    $("serverUrl").value = desktop.serverUrl || "";
    $("autoDiscover").checked = !!desktop.autoDiscover;
    $("candidateUrls").value = (desktop.candidateUrls || []).join("\n");
    $("healthcheckIntervalMs").value = desktop.healthcheckIntervalMs ?? 15000;
    $("connectTimeoutMs").value = desktop.connectTimeoutMs ?? 4000;

    $("lastfmApiKey").value = lastfm.apiKey || "";
    $("lastfmApiSecret").value = lastfm.apiSecret || "";
    $("lastfmUsername").value = lastfm.username || "";
    $("lastfmPassword").value = lastfm.password || "";

    $("debugLogs").checked = !!settings.debugLogs;
    $("sendAlbum").checked = !!settings.sendAlbum;
    $("sendDuration").checked = !!settings.sendDuration;
    $("maxQueue").value = settings.maxQueue ?? 64;
    $("maxRetries").value = settings.maxRetries ?? 10;
    $("baseRetryMs").value = settings.baseRetryMs ?? 800;
    $("maxRetryMs").value = settings.maxRetryMs ?? 30000;

    const hasSessionKey = !!String(lastfm.sessionKey || "").trim();
    const lastfmError = String(runtime?.lastfm?.lastError || "").trim();

    setSessionPill(hasSessionKey, lastfmError);

    if (mode === "standalone") {
        setLastfmValidateStatus(
            hasSessionKey
                ? "Standalone mode is ready to deliver directly to Last.fm."
                : "Validate Last.fm to create or refresh the session key.",
            false
        );
    } else {
        setLastfmValidateStatus("");
    }

    setCompanionImportHintVisible(false);

    updateModeVisibility();
}

function readForm(existingSettings = null) {
    const existingSessionKey = String(existingSettings?.lastfm?.sessionKey || "").trim();

    return {
        mode: $("modeStandalone").checked ? "standalone" : "desktop_bridge",

        desktopBridge: {
            serverUrl: $("serverUrl").value.trim(),
            autoDiscover: $("autoDiscover").checked,
            candidateUrls: $("candidateUrls").value
                .split(/\r?\n/)
                .map(s => s.trim())
                .filter(Boolean),
            healthcheckIntervalMs: Number($("healthcheckIntervalMs").value),
            connectTimeoutMs: Number($("connectTimeoutMs").value)
        },

        lastfm: {
            apiKey: $("lastfmApiKey").value.trim(),
            apiSecret: $("lastfmApiSecret").value.trim(),
            username: $("lastfmUsername").value.trim(),
            password: $("lastfmPassword").value,
            sessionKey: existingSessionKey
        },

        debugLogs: $("debugLogs").checked,
        sendAlbum: $("sendAlbum").checked,
        sendDuration: $("sendDuration").checked,
        maxQueue: Number($("maxQueue").value),
        maxRetries: Number($("maxRetries").value),
        baseRetryMs: Number($("baseRetryMs").value),
        maxRetryMs: Number($("maxRetryMs").value)
    };
}

function openLastfmApiPage() {
    chrome.tabs.create({
        url: "https://www.last.fm/api/account/create"
    });
}

async function companionHasExportableLastfm() {
    try {
        const resp = await fetch("http://127.0.0.1:5000/companion/export-lastfm", {
            method: "GET"
        });

        if (!resp.ok) return false;

        const data = await resp.json();
        return !!(
            data?.ok &&
            String(data.api_key || "").trim() &&
            String(data.api_secret || "").trim() &&
            String(data.username || "").trim() &&
            String(data.session_key || "").trim()
        );
    } catch {
        return false;
    }
}

async function refresh() {
    const resp = await getState();
    if (!resp?.ok) {
        setStatus(`Failed to load state: ${resp?.error || "unknown"}`, true);
        return null;
    }

    loadForm(resp.data.settings, resp.data.runtime);

    const mode = resp.data?.settings?.mode || "desktop_bridge";
    const lastfm = resp.data?.settings?.lastfm || {};
    const extensionLastfmEmpty = !hasAnyLastfmData(lastfm);

    if (mode === "desktop_bridge" && extensionLastfmEmpty && resp.data?.runtime?.health?.ok) {
        const canImport = await companionHasExportableLastfm();
        setCompanionImportHintVisible(canImport);
    } else {
        setCompanionImportHintVisible(false);
    }

    setStatus("Current settings loaded");
    return resp.data;
}

$("modeDesktopBridge").addEventListener("change", updateModeVisibility);
$("modeStandalone").addEventListener("change", updateModeVisibility);

$("validateLastfmBtn")?.addEventListener("click", async () => {
    const state = await getState();
    const existingSettings = state?.ok ? state.data.settings : null;
    const nextSettings = readForm(existingSettings);

    setLastfmValidateStatus("Validating Last.fm...");

    try {
        const resp = await Promise.race([
            chrome.runtime.sendMessage({
                type: "validate-lastfm",
                lastfm: nextSettings.lastfm
            }),
            new Promise((_, reject) =>
                setTimeout(() => reject(new Error("Validation timeout after 20s")), 20000)
            )
        ]);

        if (!resp?.ok) {
            setLastfmValidateStatus(`Validation failed: ${resp?.error || "unknown"}`, true);
            setSessionPill(false, resp?.error || "validation failed");
            return;
        }

        nextSettings.lastfm.sessionKey = resp.data.sessionKey;

        const saveResp = await chrome.runtime.sendMessage({
            type: "save-settings",
            settings: nextSettings
        });

        if (!saveResp?.ok) {
            setLastfmValidateStatus(`Save after validation failed: ${saveResp?.error || "unknown"}`, true);
            return;
        }

        setLastfmValidateStatus("Last.fm connection OK. Session key received.");
        setSessionPill(true, "");
        setStatus("Settings saved with Last.fm session key");
    } catch (err) {
        setLastfmValidateStatus(`Validation failed: ${String(err)}`, true);
        setSessionPill(false, String(err));
    }
});

$("reconnectLastfmBtn")?.addEventListener("click", async () => {
    setLastfmValidateStatus("Reconnecting Last.fm session...");

    const resp = await chrome.runtime.sendMessage({
        type: "reconnect-lastfm"
    });

    if (!resp?.ok) {
        setLastfmValidateStatus(`Reconnect failed: ${resp?.error || "unknown"}`, true);
        setSessionPill(false, resp?.error || "reconnect failed");
        return;
    }

    setLastfmValidateStatus("Last.fm session refreshed.");
    setSessionPill(true, "");
    setStatus("Last.fm session refreshed");

    if (resp.data?.state) {
        loadForm(resp.data.state.settings, resp.data.state.runtime);
    }
});

$("clearSessionBtn")?.addEventListener("click", async () => {
    const resp = await chrome.runtime.sendMessage({
        type: "clear-lastfm-session"
    });

    if (!resp?.ok) {
        setStatus(`Could not clear session: ${resp?.error || "unknown"}`, true);
        return;
    }

    setSessionPill(false, "");
    setLastfmValidateStatus("Session key cleared.");
    setStatus("Last.fm session key cleared");

    if (resp.data) {
        loadForm(resp.data.settings, resp.data.runtime);
    }
});

$("importFromCompanionBtn")?.addEventListener("click", async () => {
    setStatus("Importing Last.fm settings from desktop companion...");

    const resp = await chrome.runtime.sendMessage({
        type: "import-lastfm-from-companion"
    });

    if (!resp?.ok) {
        setStatus(`Import failed: ${resp?.error || "unknown"}`, true);
        return;
    }

    loadForm(resp.data.settings, resp.data.runtime);
    setCompanionImportHintVisible(false);
    setStatus("Last.fm settings imported from desktop companion");
});

$("openLastfmApiBtn")?.addEventListener("click", () => {
    openLastfmApiPage();
});

$("saveBtn").addEventListener("click", async () => {
    const current = await getState();
    const existingSettings = current?.ok ? current.data.settings : null;
    const settings = readForm(existingSettings);

    const resp = await chrome.runtime.sendMessage({
        type: "save-settings",
        settings
    });

    if (!resp?.ok) {
        setStatus(`Save failed: ${resp?.error || "unknown"}`, true);
        return;
    }

    setStatus("Settings saved");
    loadForm(resp.data.settings, resp.data.runtime);
    window.scrollTo({ top: 0, behavior: "smooth" });
});

$("reloadStateBtn").addEventListener("click", refresh);

refresh();