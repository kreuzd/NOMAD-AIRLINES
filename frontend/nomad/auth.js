/**
 * NOMAD Airlines — auth UI (inside the gallery iframe).
 *
 * Wires the login/register forms and tab switching. On success it calls back
 * into the gallery controller so it can swap to the grid view.
 */
window.NomadAuth = (function () {
	"use strict";

	function $(id) { return document.getElementById(id); }

	function showError(el, message) {
		if (!el) { return; }
		el.textContent = message || "";
		el.style.display = message ? "block" : "none";
	}

	function setTab(which) {
		var login = which !== "register";
		$("login-form").hidden = !login;
		$("register-form").hidden = login;
		$("tab-login").classList.toggle("active", login);
		$("tab-register").classList.toggle("active", !login);
		showError($("login-error"), "");
		showError($("register-error"), "");
	}

	function bind(onAuthenticated) {
		$("tab-login").addEventListener("click", function () { setTab("login"); });
		$("tab-register").addEventListener("click", function () { setTab("register"); });

		$("login-form").addEventListener("submit", function (e) {
			e.preventDefault();
			var username = $("login-username").value.trim();
			var password = $("login-password").value;
			showError($("login-error"), "");
			NomadAPI.login(username, password)
				.then(function () { onAuthenticated(); })
				.catch(function (err) { showError($("login-error"), err.error || "Login failed"); });
		});

		$("register-form").addEventListener("submit", function (e) {
			e.preventDefault();
			var username = $("reg-username").value.trim();
			var email = $("reg-email").value.trim();
			var password = $("reg-password").value;
			showError($("register-error"), "");
			NomadAPI.register(username, password, email)
				.then(function () { onAuthenticated(); })
				.catch(function (err) {
					showError($("register-error"), err.error || "Registration failed");
				});
		});
	}

	return { bind: bind, setTab: setTab, showError: showError };
})();
