/**
 * NOMAD Airlines — gallery API client (runs inside the gallery iframe).
 *
 * Thin wrapper over the backend `/api` endpoints. Persists the JWT and the
 * current user in localStorage so a session survives app restarts (one half of
 * "come back to your work"); the other half is the server-side editor state.
 */
window.NomadAPI = (function () {
	"use strict";

	var TOKEN_KEY = "nomad.token";
	var USER_KEY = "nomad.user";

	var token = localStorage.getItem(TOKEN_KEY) || null;
	var user = null;
	try {
		user = JSON.parse(localStorage.getItem(USER_KEY) || "null");
	} catch (e) {
		user = null;
	}

	function setSession(resp) {
		token = resp.access_token;
		user = resp.user;
		localStorage.setItem(TOKEN_KEY, token);
		localStorage.setItem(USER_KEY, JSON.stringify(user));
		return resp;
	}

	function clearSession() {
		token = null;
		user = null;
		localStorage.removeItem(TOKEN_KEY);
		localStorage.removeItem(USER_KEY);
	}

	/**
	 * Core request helper. Resolves with parsed JSON (or the raw Response when
	 * `opts.raw`), rejects with `{ status, error }`.
	 */
	async function request(method, path, body, opts) {
		opts = opts || {};
		var headers = {};
		if (token) { headers["Authorization"] = "Bearer " + token; }

		var fetchBody;
		if (body !== undefined) {
			headers["Content-Type"] = "application/json";
			fetchBody = JSON.stringify(body);
		}

		var res;
		try {
			res = await fetch(path, { method: method, headers: headers, body: fetchBody });
		} catch (networkError) {
			throw { status: 0, error: "Cannot reach the server. Is it running?" };
		}

		if (res.status === 401) {
			clearSession();
		}
		if (opts.raw) {
			return res;
		}

		var text = await res.text();
		var data = null;
		if (text) {
			try { data = JSON.parse(text); } catch (e) { data = null; }
		}
		if (!res.ok) {
			throw {
				status: res.status,
				error: (data && data.error) || "Request failed (HTTP " + res.status + ")",
			};
		}
		return data;
	}

	return {
		isLoggedIn: function () { return !!token; },
		currentUser: function () { return user; },

		register: function (username, password, email) {
			return request("POST", "/api/auth/register", {
				username: username,
				password: password,
				email: email || null,
			}).then(setSession);
		},
		login: function (username, password) {
			return request("POST", "/api/auth/login", {
				grant_type: "password",
				username: username,
				password: password,
			}).then(setSession);
		},
		me: function () { return request("GET", "/api/auth/me"); },
		logout: clearSession,

		listImages: function () { return request("GET", "/api/images"); },
		getImage: function (id) { return request("GET", "/api/images/" + id); },
		createImage: function (name, dataUrl, width, height) {
			return request("POST", "/api/images", {
				name: name,
				data_url: dataUrl,
				width: width != null ? width : null,
				height: height != null ? height : null,
			});
		},
		updateImage: function (id, patch) {
			return request("PUT", "/api/images/" + id, patch);
		},
		deleteImage: async function (id) {
			var res = await request("DELETE", "/api/images/" + id, undefined, { raw: true });
			if (!res.ok && res.status !== 204) {
				throw { status: res.status, error: "Failed to delete image" };
			}
			return true;
		},
		/** Fetch raw image bytes (authenticated) and return an object URL. */
		rawObjectURL: async function (id) {
			var res = await request("GET", "/api/images/" + id + "/raw", undefined, { raw: true });
			if (!res.ok) {
				throw { status: res.status, error: "Failed to load image" };
			}
			var blob = await res.blob();
			return URL.createObjectURL(blob);
		},

		getState: function () { return request("GET", "/api/state"); },
		setState: function (state) { return request("PUT", "/api/state", { state: state }); },
	};
})();
