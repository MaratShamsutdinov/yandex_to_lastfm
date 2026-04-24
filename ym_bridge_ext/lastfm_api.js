(function () {
    const API_URL = "https://ws.audioscrobbler.com/2.0/";
    const EMBEDDED_LASTFM_API_KEY = "82486b94f8525fe02a64e11aef66da13";
    const EMBEDDED_LASTFM_API_SECRET = "7130cd81905f53c114c96b7efe98580f";

    function normalizeLastfmSettings(lastfm) {
        const rawApiKey = String(lastfm?.apiKey || "").trim();
        const rawApiSecret = String(lastfm?.apiSecret || "").trim();

        return {
            apiKey: rawApiKey || EMBEDDED_LASTFM_API_KEY,
            apiSecret: rawApiSecret || EMBEDDED_LASTFM_API_SECRET,
            sessionKey: String(lastfm?.sessionKey || "").trim()
        };
    }
    function isLastfmConfigComplete(lastfm) {
        const v = normalizeLastfmSettings(lastfm);
        return !!(v.apiKey && v.apiSecret && v.sessionKey);
    }

    function hasStandaloneRuntimeAuth(lastfm) {
        const v = normalizeLastfmSettings(lastfm);
        return !!(v.apiKey && v.apiSecret && v.sessionKey);
    }

    function encodeUtf8(str) {
        return new TextEncoder().encode(str);
    }

    async function md5Hex(str) {
        function cmn(q, a, b, x, s, t) {
            a = (((a + q) | 0) + ((x + t) | 0)) | 0;
            return (((a << s) | (a >>> (32 - s))) + b) | 0;
        }

        function ff(a, b, c, d, x, s, t) {
            return cmn((b & c) | (~b & d), a, b, x, s, t);
        }

        function gg(a, b, c, d, x, s, t) {
            return cmn((b & d) | (c & ~d), a, b, x, s, t);
        }

        function hh(a, b, c, d, x, s, t) {
            return cmn(b ^ c ^ d, a, b, x, s, t);
        }

        function ii(a, b, c, d, x, s, t) {
            return cmn(c ^ (b | ~d), a, b, x, s, t);
        }

        function md5cycle(state, x) {
            let [a, b, c, d] = state;

            a = ff(a, b, c, d, x[0], 7, -680876936);
            d = ff(d, a, b, c, x[1], 12, -389564586);
            c = ff(c, d, a, b, x[2], 17, 606105819);
            b = ff(b, c, d, a, x[3], 22, -1044525330);
            a = ff(a, b, c, d, x[4], 7, -176418897);
            d = ff(d, a, b, c, x[5], 12, 1200080426);
            c = ff(c, d, a, b, x[6], 17, -1473231341);
            b = ff(b, c, d, a, x[7], 22, -45705983);
            a = ff(a, b, c, d, x[8], 7, 1770035416);
            d = ff(d, a, b, c, x[9], 12, -1958414417);
            c = ff(c, d, a, b, x[10], 17, -42063);
            b = ff(b, c, d, a, x[11], 22, -1990404162);
            a = ff(a, b, c, d, x[12], 7, 1804603682);
            d = ff(d, a, b, c, x[13], 12, -40341101);
            c = ff(c, d, a, b, x[14], 17, -1502002290);
            b = ff(b, c, d, a, x[15], 22, 1236535329);

            a = gg(a, b, c, d, x[1], 5, -165796510);
            d = gg(d, a, b, c, x[6], 9, -1069501632);
            c = gg(c, d, a, b, x[11], 14, 643717713);
            b = gg(b, c, d, a, x[0], 20, -373897302);
            a = gg(a, b, c, d, x[5], 5, -701558691);
            d = gg(d, a, b, c, x[10], 9, 38016083);
            c = gg(c, d, a, b, x[15], 14, -660478335);
            b = gg(b, c, d, a, x[4], 20, -405537848);
            a = gg(a, b, c, d, x[9], 5, 568446438);
            d = gg(d, a, b, c, x[14], 9, -1019803690);
            c = gg(c, d, a, b, x[3], 14, -187363961);
            b = gg(b, c, d, a, x[8], 20, 1163531501);
            a = gg(a, b, c, d, x[13], 5, -1444681467);
            d = gg(d, a, b, c, x[2], 9, -51403784);
            c = gg(c, d, a, b, x[7], 14, 1735328473);
            b = gg(b, c, d, a, x[12], 20, -1926607734);

            a = hh(a, b, c, d, x[5], 4, -378558);
            d = hh(d, a, b, c, x[8], 11, -2022574463);
            c = hh(c, d, a, b, x[11], 16, 1839030562);
            b = hh(b, c, d, a, x[14], 23, -35309556);
            a = hh(a, b, c, d, x[1], 4, -1530992060);
            d = hh(d, a, b, c, x[4], 11, 1272893353);
            c = hh(c, d, a, b, x[7], 16, -155497632);
            b = hh(b, c, d, a, x[10], 23, -1094730640);
            a = hh(a, b, c, d, x[13], 4, 681279174);
            d = hh(d, a, b, c, x[0], 11, -358537222);
            c = hh(c, d, a, b, x[3], 16, -722521979);
            b = hh(b, c, d, a, x[6], 23, 76029189);
            a = hh(a, b, c, d, x[9], 4, -640364487);
            d = hh(d, a, b, c, x[12], 11, -421815835);
            c = hh(c, d, a, b, x[15], 16, 530742520);
            b = hh(b, c, d, a, x[2], 23, -995338651);

            a = ii(a, b, c, d, x[0], 6, -198630844);
            d = ii(d, a, b, c, x[7], 10, 1126891415);
            c = ii(c, d, a, b, x[14], 15, -1416354905);
            b = ii(b, c, d, a, x[5], 21, -57434055);
            a = ii(a, b, c, d, x[12], 6, 1700485571);
            d = ii(d, a, b, c, x[3], 10, -1894986606);
            c = ii(c, d, a, b, x[10], 15, -1051523);
            b = ii(b, c, d, a, x[1], 21, -2054922799);
            a = ii(a, b, c, d, x[8], 6, 1873313359);
            d = ii(d, a, b, c, x[15], 10, -30611744);
            c = ii(c, d, a, b, x[6], 15, -1560198380);
            b = ii(b, c, d, a, x[13], 21, 1309151649);
            a = ii(a, b, c, d, x[4], 6, -145523070);
            d = ii(d, a, b, c, x[11], 10, -1120210379);
            c = ii(c, d, a, b, x[2], 15, 718787259);
            b = ii(b, c, d, a, x[9], 21, -343485551);

            state[0] = (state[0] + a) | 0;
            state[1] = (state[1] + b) | 0;
            state[2] = (state[2] + c) | 0;
            state[3] = (state[3] + d) | 0;
        }

        function md5blk(s) {
            const md5blks = [];
            for (let i = 0; i < 64; i += 4) {
                md5blks[i >> 2] =
                    s.charCodeAt(i) +
                    (s.charCodeAt(i + 1) << 8) +
                    (s.charCodeAt(i + 2) << 16) +
                    (s.charCodeAt(i + 3) << 24);
            }
            return md5blks;
        }

        function md51(s) {
            const n = s.length;
            const state = [1732584193, -271733879, -1732584194, 271733878];
            let i;

            for (i = 64; i <= n; i += 64) {
                md5cycle(state, md5blk(s.substring(i - 64, i)));
            }

            s = s.substring(i - 64);
            const tail = new Array(16).fill(0);

            for (i = 0; i < s.length; i += 1) {
                tail[i >> 2] |= s.charCodeAt(i) << ((i % 4) << 3);
            }

            tail[i >> 2] |= 0x80 << ((i % 4) << 3);

            if (i > 55) {
                md5cycle(state, tail);
                for (let j = 0; j < 16; j += 1) tail[j] = 0;
            }

            tail[14] = n * 8;
            md5cycle(state, tail);

            return state;
        }

        function rhex(n) {
            const s = "0123456789abcdef";
            let j;
            let out = "";
            for (j = 0; j < 4; j += 1) {
                out += s.charAt((n >> (j * 8 + 4)) & 0x0f) + s.charAt((n >> (j * 8)) & 0x0f);
            }
            return out;
        }

        function hex(x) {
            return x.map(rhex).join("");
        }

        return hex(md51(unescape(encodeURIComponent(str))));
    }

    async function buildApiSig(params, apiSecret) {
        const keys = Object.keys(params)
            .filter(k => k !== "format" && k !== "callback" && k !== "api_sig")
            .sort();

        let raw = "";
        for (const key of keys) {
            raw += key;
            raw += String(params[key] ?? "");
        }

        raw += String(apiSecret || "").trim();
        return await md5Hex(raw);
    }

    async function postLastfm(lastfm, params) {
        const cfg = normalizeLastfmSettings(lastfm);

        if (!cfg.apiKey || !cfg.apiSecret) {
            throw new Error("Last.fm API Key / API Secret missing");
        }

        const body = {
            ...params,
            api_key: params.api_key || cfg.apiKey
        };

        body.api_sig = await buildApiSig(body, cfg.apiSecret);
        body.format = "json";

        const form = new URLSearchParams();
        for (const [k, v] of Object.entries(body)) {
            if (v == null) continue;
            form.set(k, String(v));
        }

        const resp = await fetch(API_URL, {
            method: "POST",
            headers: {
                "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8"
            },
            body: form.toString()
        });

        const text = await resp.text();

        if (!resp.ok) {
            throw new Error(`HTTP ${resp.status}: ${text}`);
        }

        let parsed;
        try {
            parsed = JSON.parse(text);
        } catch (e) {
            throw new Error(`JSON parse error: ${String(e)}; body=${text}`);
        }

        if (parsed && parsed.error) {
            throw new Error(`Last.fm error ${parsed.error}: ${parsed.message || "Unknown error"}`);
        }

        return parsed;
    }

    function buildLastfmAuthUrl(lastfm, token) {
        const cfg = normalizeLastfmSettings(lastfm);
        return `https://www.last.fm/api/auth/?api_key=${encodeURIComponent(cfg.apiKey)}&token=${encodeURIComponent(token)}`;
    }

    async function getAuthToken(lastfm) {
        const cfg = normalizeLastfmSettings(lastfm);

        if (!cfg.apiKey || !cfg.apiSecret) {
            throw new Error("Last.fm API Key / API Secret missing");
        }

        const result = await postLastfm(cfg, {
            method: "auth.getToken",
            api_key: cfg.apiKey
        });

        const token = result?.token;
        if (!token) {
            throw new Error(`Could not get Last.fm auth token: ${JSON.stringify(result)}`);
        }

        return token;
    }

    async function getSessionKeyFromToken(lastfm, token) {
        const cfg = normalizeLastfmSettings(lastfm);
        const cleanToken = String(token || "").trim();

        if (!cfg.apiKey || !cfg.apiSecret) {
            throw new Error("Last.fm API Key / API Secret missing");
        }

        if (!cleanToken) {
            throw new Error("Last.fm auth token missing");
        }

        const result = await postLastfm(cfg, {
            method: "auth.getSession",
            token: cleanToken,
            api_key: cfg.apiKey
        });

        const sessionKey = result?.session?.key;
        if (!sessionKey) {
            throw new Error(`Could not get Last.fm session key from token: ${JSON.stringify(result)}`);
        }

        return sessionKey;
    }

    async function validateSessionKey(lastfm) {
        const cfg = normalizeLastfmSettings(lastfm);
        const sk = String(cfg.sessionKey || "").trim();

        if (!sk) {
            throw new Error("Last.fm session key missing");
        }

        return await postLastfm(cfg, {
            method: "user.getInfo",
            sk,
            api_key: cfg.apiKey
        });
    }

    async function updateNowPlaying(lastfm, artist, track, album = "", duration = null) {
        const cfg = normalizeLastfmSettings(lastfm);
        const sk = cfg.sessionKey;

        if (!sk) {
            throw new Error("Last.fm session key missing");
        }

        const params = {
            method: "track.updateNowPlaying",
            artist: String(artist || "").trim(),
            track: String(track || "").trim(),
            sk,
            api_key: cfg.apiKey
        };

        const safeAlbum = String(album || "").trim();
        if (safeAlbum) {
            params.album = safeAlbum;
        }

        if (Number.isFinite(duration) && duration > 0) {
            params.duration = Math.round(duration).toString();
        }

        return await postLastfm(cfg, params);
    }

    async function scrobble(lastfm, artist, track, timestamp, album = "", duration = null) {
        const cfg = normalizeLastfmSettings(lastfm);
        const sk = cfg.sessionKey;

        if (!sk) {
            throw new Error("Last.fm session key missing");
        }

        const params = {
            method: "track.scrobble",
            artist: String(artist || "").trim(),
            track: String(track || "").trim(),
            timestamp: String(timestamp),
            sk,
            api_key: cfg.apiKey
        };

        const safeAlbum = String(album || "").trim();
        if (safeAlbum) {
            params.album = safeAlbum;
        }

        if (Number.isFinite(duration) && duration > 0) {
            params.duration = Math.round(duration).toString();
        }

        return await postLastfm(cfg, params);
    }

    globalThis.YMBridgeLastfmApi = {
        normalizeLastfmSettings,
        isLastfmConfigComplete,
        hasStandaloneRuntimeAuth,
        buildApiSig,
        postLastfm,
        getAuthToken,
        getSessionKeyFromToken,
        buildLastfmAuthUrl,
        validateSessionKey,
        updateNowPlaying,
        scrobble
    };
})();