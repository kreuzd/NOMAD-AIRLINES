/**
 * NOMAD Airlines — jspaint integration (runs in the PARENT window).
 *
 * Responsibilities:
 *   1. Add a "Gallery" button to jspaint's nav/menu bar (`.menus`), styled like
 *      a native menu button so it matches the base project.
 *   2. Host the gallery UI in a same-origin <iframe> (so Bootstrap's global CSS
 *      reset can't disturb jspaint's layout) and toggle it as an overlay.
 *   3. Expose `window.NomadBridge` so the iframe can read the current canvas
 *      (export/save) and load an image back into the editor (open/edit).
 *
 * Loaded as a classic script, so it may run before jspaint's deferred modules
 * finish — everything is therefore guarded behind readiness polling.
 */
(function () {
	"use strict";

	/** Poll until `check()` is truthy, then call `cb`. Gives up after `tries`. */
	function waitFor(check, cb, tries) {
		tries = tries == null ? 300 : tries;
		if (check()) { cb(); return; }
		if (tries <= 0) { return; }
		setTimeout(function () { waitFor(check, cb, tries - 1); }, 100);
	}

	function getCanvas() {
		return document.querySelector("canvas.main-canvas");
	}

	// --- Bridge: the only surface the gallery iframe touches in the parent ---
	var NomadBridge = {
		/** Current drawing as a PNG data URL, or null if the canvas isn't ready. */
		getCanvasDataURL: function () {
			var canvas = getCanvas();
			if (!canvas) { return null; }
			try {
				return canvas.toDataURL("image/png");
			} catch (e) {
				return null;
			}
		},
		getCanvasSize: function () {
			var canvas = getCanvas();
			return canvas
				? { width: canvas.width, height: canvas.height }
				: { width: null, height: null };
		},
		/**
		 * Load an image (given as a data URL) into the jspaint editor using its
		 * own `open_from_file` path, so history/undo behave normally.
		 */
		loadImageDataURL: function (name, dataURL) {
			return fetch(dataURL)
				.then(function (r) { return r.blob(); })
				.then(function (blob) {
					var safeName = /\.(png|jpe?g|gif|bmp|webp)$/i.test(name)
						? name
						: name + ".png";
					var file = new File([blob], safeName, {
						type: blob.type || "image/png",
					});
					if (typeof window.open_from_file === "function") {
						window.open_from_file(file);
						return true;
					}
					console.warn("[nomad] open_from_file unavailable");
					return false;
				});
		},
		isEditorReady: function () {
			return typeof window.open_from_file === "function" && !!getCanvas();
		},
		showGallery: showOverlay,
		closeGallery: hideOverlay,
	};
	window.NomadBridge = NomadBridge;

	// --- Overlay + iframe ----------------------------------------------------
	var overlay = null;
	var iframe = null;

	function buildOverlay() {
		if (overlay) { return; }
		overlay = document.createElement("div");
		overlay.id = "nomad-gallery-overlay";

		iframe = document.createElement("iframe");
		iframe.id = "nomad-gallery-frame";
		iframe.title = "NOMAD Airlines Gallery";
		iframe.src = "nomad/gallery.html";
		overlay.appendChild(iframe);

		// Click on the dimmed backdrop (outside the frame) closes the gallery.
		overlay.addEventListener("click", function (e) {
			if (e.target === overlay) { hideOverlay(); }
		});
		document.body.appendChild(overlay);
	}

	function showOverlay() {
		buildOverlay();
		overlay.classList.add("open");
		// Ask the iframe to refresh its view (login state / grid).
		try {
			iframe.contentWindow.postMessage(
				{ type: "nomad:open" },
				window.location.origin
			);
		} catch (e) { /* iframe not ready yet; it refreshes on its own load */ }
	}

	function hideOverlay() {
		if (overlay) { overlay.classList.remove("open"); }
	}

	// --- Nav menu button -----------------------------------------------------
	function addMenuButton() {
		var menus = document.querySelector(".menus");
		if (!menus || document.getElementById("nomad-gallery-menu-button")) {
			return;
		}
		var btn = document.createElement("div");
		btn.className = "menu-button nomad-gallery-menu-button";
		btn.id = "nomad-gallery-menu-button";
		btn.setAttribute("role", "menuitem");
		btn.tabIndex = -1;
		// Mirror jspaint's `<span>label</span>` markup for consistent styling.
		btn.innerHTML = "<span>🖼️&nbsp;Gallery</span>";
		btn.addEventListener("click", showOverlay);
		btn.addEventListener("keydown", function (e) {
			if (e.key === "Enter" || e.key === " ") {
				e.preventDefault();
				showOverlay();
			}
		});
		menus.appendChild(btn);
	}

	// Allow the iframe to message the parent (close, loaded notifications).
	window.addEventListener("message", function (e) {
		if (e.origin !== window.location.origin || !e.data) { return; }
		if (e.data.type === "nomad:close") { hideOverlay(); }
	});

	// Escape closes the gallery overlay.
	window.addEventListener("keydown", function (e) {
		if (e.key === "Escape" && overlay && overlay.classList.contains("open")) {
			hideOverlay();
		}
	});

	function boot() {
		buildOverlay();
		waitFor(function () { return document.querySelector(".menus"); }, addMenuButton);
	}

	if (document.readyState === "loading") {
		document.addEventListener("DOMContentLoaded", boot);
	} else {
		boot();
	}
})();
