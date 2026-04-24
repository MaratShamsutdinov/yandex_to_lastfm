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

function cleanErrorMessage(err) {
    return String(err || "")
        .replace(/^Error:\s*/i, "")
        .replace(/^Error:\s*/i, "")
        .trim();
}

function setSessionPill(hasSessionKey, lastfmError = "", lastfmConnected = false) {
    const pill = $("lastfmSessionPill");
    if (!pill) return;

    pill.className = "pill";

    if (lastfmError) {
        pill.textContent = "Session: error";
        pill.classList.add("bad");
        return;
    }

    if (hasSessionKey && lastfmConnected) {
        pill.textContent = "Session: connected";
        pill.classList.add("ok");
        return;
    }

    if (hasSessionKey) {
        pill.textContent = "Session: saved";
        pill.classList.add("warn");
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
    return !!String(lastfm.sessionKey || "").trim();
}

function loadForm(settings, runtime = null) {
    const mode = settings?.mode || "standalone";
    $("modeStandalone").checked = mode === "standalone";
    $("modeDesktopBridge").checked = mode === "desktop_bridge";

    const desktop = settings?.desktopBridge || {};
    const lastfm = settings?.lastfm || {};

    $("serverUrl").value = desktop.serverUrl || "";
    $("autoDiscover").checked = !!desktop.autoDiscover;
    $("candidateUrls").value = (desktop.candidateUrls || []).join("\n");
    $("healthcheckIntervalMs").value = desktop.healthcheckIntervalMs ?? 15000;
    $("connectTimeoutMs").value = desktop.connectTimeoutMs ?? 4000;

    $("lastfmSessionKey").value = lastfm.sessionKey || "";

    $("debugLogs").checked = !!settings.debugLogs;
    $("sendAlbum").checked = !!settings.sendAlbum;
    $("sendDuration").checked = !!settings.sendDuration;
    $("maxQueue").value = settings.maxQueue ?? 64;
    $("maxRetries").value = settings.maxRetries ?? 10;
    $("baseRetryMs").value = settings.baseRetryMs ?? 800;
    $("maxRetryMs").value = settings.maxRetryMs ?? 30000;

    const hasSessionKey = !!String(lastfm.sessionKey || "").trim();
    const lastfmError = String(runtime?.lastfm?.lastError || "").trim();
    const lastfmConnected = !!runtime?.lastfm?.connected;

    setSessionPill(hasSessionKey, lastfmError, lastfmConnected);

    if (mode === "standalone") {
        setLastfmValidateStatus(
            lastfmError
                ? cleanErrorMessage(lastfmError)
                : lastfmConnected
                    ? "Standalone mode is connected to Last.fm."
                    : hasSessionKey
                        ? "Session key is saved. It will be validated on the next Last.fm request."
                        : "Connect Last.fm, import a session from the desktop companion, or paste a session key manually.",
            !!lastfmError
        );
    } else {
        setLastfmValidateStatus("");
    }

    setCompanionImportHintVisible(false);

    updateModeVisibility();
}

function readForm(existingSettings = null) {
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
            sessionKey: $("lastfmSessionKey").value.trim()
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


$("connectLastfmBtn")?.addEventListener("click", async () => {
    try {
        setLastfmValidateStatus("Opening Last.fm approval page...");
        const resp = await chrome.runtime.sendMessage({
            type: "connect-lastfm-browser-auth"
        });

        if (!resp?.ok) {
            throw new Error(resp?.error || "Last.fm connect failed");
        }

        const state = await getState();
        loadForm(state.data.settings, state.data.runtime);
        setLastfmValidateStatus("Last.fm connected successfully.");
        setStatus("Last.fm connected");
    } catch (err) {
        setLastfmValidateStatus(String(err), true);
    }
});

$("reconnectLastfmBtn")?.addEventListener("click", async () => {
    try {
        setLastfmValidateStatus("Reconnecting Last.fm...");
        const resp = await chrome.runtime.sendMessage({
            type: "reconnect-lastfm"
        });

        if (!resp?.ok) {
            throw new Error(resp?.error || "Reconnect failed");
        }

        const state = await getState();
        loadForm(state.data.settings, state.data.runtime);
        setLastfmValidateStatus("Last.fm session refreshed.");
        setStatus("Last.fm reconnected");
    } catch (err) {
        setLastfmValidateStatus(String(err), true);
    }
});

$("clearSessionBtn")?.addEventListener("click", async () => {
    try {
        const resp = await chrome.runtime.sendMessage({
            type: "clear-lastfm-session"
        });

        if (!resp?.ok) {
            throw new Error(resp?.error || "Could not clear session key");
        }

        const state = await getState();
        loadForm(state.data.settings, state.data.runtime);
        setSessionPill(false, "", false);
        setLastfmValidateStatus("Session key cleared.");
        setStatus("Session key cleared");
    } catch (err) {
        setLastfmValidateStatus(String(err), true);
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

$("importFromCompanionBtn2")?.addEventListener("click", async () => {
    try {
        setLastfmValidateStatus("Checking desktop companion and importing Last.fm session...");
        const resp = await chrome.runtime.sendMessage({
            type: "import-lastfm-from-companion"
        });

        if (!resp?.ok) {
            throw new Error(resp?.error || "Import failed");
        }

        const state = await getState();
        loadForm(state.data.settings, state.data.runtime);
        setLastfmValidateStatus("Last.fm session imported successfully.");
        setStatus("Imported from desktop companion");
    } catch (err) {
        setLastfmValidateStatus(cleanErrorMessage(err), true);
    }
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