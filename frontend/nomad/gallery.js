/**
 * NOMAD Airlines — gallery controller (inside the gallery iframe).
 *
 * Orchestrates auth vs. grid views and implements the gallery features:
 *   - create  : save the current jspaint canvas as a new gallery image
 *   - import  : from device (file picker) → gallery, or from gallery → editor
 *   - export  : to device (download) or to gallery (save canvas back)
 *   - manage  : open/edit, rename, delete
 *   - state   : remember the last opened image so the user resumes their work
 */
(function () {
	"use strict";

	function $(id) { return document.getElementById(id); }

	/** The parent-window bridge into jspaint; null if opened standalone. */
	var bridge = (window.parent && window.parent !== window)
		? window.parent.NomadBridge
		: null;

	var objectURLs = [];
	function trackURL(url) { objectURLs.push(url); return url; }
	function revokeURLs() {
		objectURLs.forEach(function (u) { URL.revokeObjectURL(u); });
		objectURLs = [];
	}

	// --- view switching ------------------------------------------------------
	function showSection(name) {
		$("auth-section").hidden = name !== "auth";
		$("gallery-section").hidden = name !== "gallery";
	}

	function refreshUserArea() {
		var user = NomadAPI.currentUser();
		$("user-area").textContent = user ? "Signed in as " + user.username : "";
		$("logout-btn").hidden = !user;
	}

	var statusTimer = null;
	function toast(message, kind) {
		var box = $("gallery-status");
		box.innerHTML = "";
		if (!message) { return; }
		var div = document.createElement("div");
		div.className = "alert alert-" + (kind || "info") + " py-2";
		div.textContent = message;
		box.appendChild(div);
		clearTimeout(statusTimer);
		statusTimer = setTimeout(function () { box.innerHTML = ""; }, 4000);
	}

	function canUseCanvas() {
		return bridge && bridge.isEditorReady && bridge.isEditorReady();
	}

	function formatBytes(n) {
		if (n < 1024) { return n + " B"; }
		if (n < 1024 * 1024) { return (n / 1024).toFixed(1) + " KB"; }
		return (n / 1024 / 1024).toFixed(1) + " MB";
	}

	// --- grid rendering ------------------------------------------------------
	function makeCard(img) {
		var col = document.createElement("div");
		col.className = "col";

		var card = document.createElement("div");
		card.className = "card h-100";

		var thumb = document.createElement("div");
		thumb.className = "nomad-thumb loading";
		card.appendChild(thumb);

		var bodyEl = document.createElement("div");
		bodyEl.className = "card-body p-2";

		var title = document.createElement("h6");
		title.className = "card-title nomad-name mb-1";
		title.textContent = img.name;          // textContent → no HTML injection
		title.title = img.name;
		bodyEl.appendChild(title);

		var meta = document.createElement("div");
		meta.className = "nomad-meta mb-2";
		var dims = (img.width && img.height) ? img.width + "×" + img.height + " · " : "";
		meta.textContent = dims + formatBytes(img.size);
		bodyEl.appendChild(meta);

		var actions = document.createElement("div");
		actions.className = "d-flex flex-wrap gap-1";
		actions.appendChild(button("Open", "btn-primary", function () { openInEditor(img); },
			canUseCanvas() ? "" : "Open the editor to use this"));
		actions.appendChild(button("Save canvas", "btn-success", function () { saveCanvasHere(img); },
			canUseCanvas() ? "" : "Editor not available"));
		actions.appendChild(button("Download", "btn-outline-secondary", function () { downloadImage(img); }));
		actions.appendChild(button("PDF", "btn-outline-secondary", function () { downloadAsPDF(img); }));
		actions.appendChild(button("Rename", "btn-outline-secondary", function () { renameImage(img); }));
		actions.appendChild(button("Delete", "btn-outline-danger", function () { deleteImage(img); }));
		bodyEl.appendChild(actions);

		card.appendChild(bodyEl);
		col.appendChild(card);

		// Lazily load the thumbnail via authenticated fetch → object URL.
		NomadAPI.rawObjectURL(img.id)
			.then(function (url) {
				var el = document.createElement("img");
				el.alt = img.name;
				el.src = trackURL(url);
				thumb.classList.remove("loading");
				thumb.appendChild(el);
			})
			.catch(function () { thumb.classList.remove("loading"); thumb.textContent = "⚠"; });

		return col;
	}

	function button(label, variant, handler, disabledReason) {
		var b = document.createElement("button");
		b.type = "button";
		b.className = "btn btn-sm " + variant;
		b.textContent = label;
		if (disabledReason) {
			b.disabled = true;
			b.title = disabledReason;
		} else {
			b.addEventListener("click", handler);
		}
		return b;
	}

	function renderGrid() {
		return NomadAPI.listImages().then(function (images) {
			revokeURLs();
			var grid = $("gallery-grid");
			grid.innerHTML = "";
			$("empty-state").hidden = images.length > 0;
			$("gallery-count").textContent = images.length
				? images.length + (images.length === 1 ? " image" : " images")
				: "";
			images.forEach(function (img) { grid.appendChild(makeCard(img)); });
		}).catch(function (err) { toast(err.error || "Failed to load gallery", "danger"); });
	}

	// --- actions -------------------------------------------------------------
	function openInEditor(img) {
		if (!canUseCanvas()) { toast("Editor is not available", "warning"); return; }
		NomadAPI.getImage(img.id)
			.then(function (full) { return bridge.loadImageDataURL(full.name, full.data_url); })
			.then(function () { return NomadAPI.setState({ openImageId: img.id, name: img.name, ts: Date.now() }); })
			.then(function () { closeGallery(); })
			.catch(function (err) { toast(err.error || "Failed to open image", "danger"); });
	}

	function saveCanvasHere(img) {
		var dataURL = bridge && bridge.getCanvasDataURL && bridge.getCanvasDataURL();
		if (!dataURL) { toast("Could not read the canvas", "warning"); return; }
		var size = bridge.getCanvasSize ? bridge.getCanvasSize() : {};
		NomadAPI.updateImage(img.id, { data_url: dataURL, width: size.width, height: size.height })
			.then(function () { toast('Saved canvas into "' + img.name + '"', "success"); return renderGrid(); })
			.then(function () { return NomadAPI.setState({ openImageId: img.id, name: img.name, ts: Date.now() }); })
			.catch(function (err) { toast(err.error || "Save failed", "danger"); });
	}

	function downloadImage(img) {
		NomadAPI.getImage(img.id).then(function (full) {
			var a = document.createElement("a");
			a.href = full.data_url;
			a.download = /\.[a-z0-9]+$/i.test(img.name) ? img.name : img.name + ".png";
			document.body.appendChild(a);
			a.click();
			a.remove();
		}).catch(function (err) { toast(err.error || "Download failed", "danger"); });
	}

	function downloadAsPDF(img) {
		NomadAPI.getImage(img.id).then(function (full) {
			var image = new Image();
			image.onload = function () {
				var w = image.naturalWidth;
				var h = image.naturalHeight;
				var canvas = document.createElement("canvas");
				canvas.width = w;
				canvas.height = h;
				canvas.getContext("2d").drawImage(image, 0, 0);

				var jpegStr = atob(canvas.toDataURL("image/jpeg", 0.92).split(",")[1]);
				var jpegLen = jpegStr.length;
				var enc = new TextEncoder();
				var parts = [];
				var pos = 0;
				var off = {};

				function pushStr(s) { var b = enc.encode(s); parts.push(b); pos += b.length; }
				function pushBin(s) {
					var b = new Uint8Array(s.length);
					for (var i = 0; i < s.length; i++) { b[i] = s.charCodeAt(i); }
					parts.push(b); pos += b.length;
				}
				function zpad(n, l) { var s = "" + n; while (s.length < l) { s = "0" + s; } return s; }

				pushStr("%PDF-1.4\n");

				off[1] = pos;
				pushStr("1 0 obj\n<</Type /XObject /Subtype /Image /Width " + w + " /Height " + h +
					" /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length " + jpegLen + ">>\nstream\n");
				pushBin(jpegStr);
				pushStr("\nendstream\nendobj\n");

				off[2] = pos;
				pushStr("2 0 obj\n<</XObject <</Im1 1 0 R>>>>\nendobj\n");

				var content = "q " + w + " 0 0 " + h + " 0 0 cm /Im1 Do Q";

				off[3] = pos;
				pushStr("3 0 obj\n<</Type /Page /Parent 4 0 R /MediaBox [0 0 " + w + " " + h +
					"] /Contents 5 0 R /Resources 2 0 R>>\nendobj\n");

				off[4] = pos;
				pushStr("4 0 obj\n<</Type /Pages /Kids [3 0 R] /Count 1>>\nendobj\n");

				off[5] = pos;
				pushStr("5 0 obj\n<</Length " + content.length + ">>\nstream\n" + content + "\nendstream\nendobj\n");

				off[6] = pos;
				pushStr("6 0 obj\n<</Type /Catalog /Pages 4 0 R>>\nendobj\n");

				var xrefPos = pos;
				pushStr("xref\n0 7\n0000000000 65535 f \n");
				for (var k = 1; k <= 6; k++) { pushStr(zpad(off[k], 10) + " 00000 n \n"); }
				pushStr("trailer\n<</Size 7 /Root 6 0 R>>\nstartxref\n" + xrefPos + "\n%%EOF");

				var total = parts.reduce(function (s, p) { return s + p.length; }, 0);
				var buf = new Uint8Array(total);
				var cursor = 0;
				parts.forEach(function (p) { buf.set(p, cursor); cursor += p.length; });

				var blob = new Blob([buf], { type: "application/pdf" });
				var url = URL.createObjectURL(blob);
				var a = document.createElement("a");
				a.href = url;
				a.download = img.name.replace(/\.[^.]+$/, "") + ".pdf";
				document.body.appendChild(a);
				a.click();
				a.remove();
				setTimeout(function () { URL.revokeObjectURL(url); }, 1000);
			};
			image.onerror = function () { toast("Could not load image for PDF export", "danger"); };
			image.src = full.data_url;
		}).catch(function (err) { toast(err.error || "PDF export failed", "danger"); });
	}

	function renameImage(img) {
		var name = window.prompt("Rename image:", img.name);
		if (name == null) { return; }
		name = name.trim();
		if (!name) { toast("Name cannot be empty", "warning"); return; }
		NomadAPI.updateImage(img.id, { name: name })
			.then(function () { return renderGrid(); })
			.catch(function (err) { toast(err.error || "Rename failed", "danger"); });
	}

	function deleteImage(img) {
		if (!window.confirm('Delete "' + img.name + '"? This cannot be undone.')) { return; }
		NomadAPI.deleteImage(img.id)
			.then(function () { toast("Deleted", "success"); return renderGrid(); })
			.catch(function (err) { toast(err.error || "Delete failed", "danger"); });
	}

	// toolbar: save current canvas as a NEW image
	function saveCurrentAsNew() {
		var dataURL = bridge && bridge.getCanvasDataURL && bridge.getCanvasDataURL();
		if (!dataURL) { toast("Open the editor first, then draw something to save", "warning"); return; }
		var name = window.prompt("Name this drawing:", "Untitled");
		if (name == null) { return; }
		name = name.trim() || "Untitled";
		var size = bridge.getCanvasSize ? bridge.getCanvasSize() : {};
		NomadAPI.createImage(name, dataURL, size.width, size.height)
			.then(function (meta) {
				toast('Saved "' + name + '" to the gallery', "success");
				return NomadAPI.setState({ openImageId: meta.id, name: name, ts: Date.now() })
					.then(renderGrid);
			})
			.catch(function (err) { toast(err.error || "Save failed", "danger"); });
	}

	// toolbar: import an image file from the device INTO the gallery
	function importFromDevice(file) {
		if (!file) { return; }
		var reader = new FileReader();
		reader.onload = function () {
			var dataURL = reader.result;
			var defaultName = file.name.replace(/\.[^.]+$/, "") || "Imported";
			var name = window.prompt("Name this imported image:", defaultName);
			if (name == null) { return; }
			name = name.trim() || defaultName;
			NomadAPI.createImage(name, dataURL, null, null)
				.then(function () { toast('Imported "' + name + '"', "success"); return renderGrid(); })
				.catch(function (err) { toast(err.error || "Import failed", "danger"); });
		};
		reader.onerror = function () { toast("Could not read file", "danger"); };
		reader.readAsDataURL(file);
	}

	// --- resume last work ----------------------------------------------------
	function maybeResume() {
		return NomadAPI.getState().then(function (resp) {
			var st = resp && resp.state;
			if (st && st.openImageId && canUseCanvas()) {
				$("resume-text").textContent = 'Resume your last drawing: "' + (st.name || "image") + '"?';
				$("resume-banner").hidden = false;
				$("resume-btn").onclick = function () {
					$("resume-banner").hidden = true;
					openInEditor({ id: st.openImageId, name: st.name || "image" });
				};
				$("resume-dismiss").onclick = function () { $("resume-banner").hidden = true; };
			} else {
				$("resume-banner").hidden = true;
			}
		}).catch(function () { /* state is best-effort */ });
	}

	function closeGallery() {
		if (bridge && bridge.closeGallery) {
			bridge.closeGallery();
		} else {
			try { window.parent.postMessage({ type: "nomad:close" }, window.location.origin); } catch (e) { /* */ }
		}
	}

	// --- init ----------------------------------------------------------------
	function enterGallery() {
		showSection("gallery");
		refreshUserArea();
		if (!canUseCanvas()) {
			toast("Tip: open this from inside the editor to save/open drawings directly.", "secondary");
		}
		return renderGrid().then(maybeResume);
	}

	function init() {
		// If we hold a token, validate it; drop it silently if expired.
		var ready = NomadAPI.isLoggedIn()
			? NomadAPI.me().then(function () { return true; }).catch(function () { NomadAPI.logout(); return false; })
			: Promise.resolve(false);

		ready.then(function (loggedIn) {
			if (loggedIn) {
				enterGallery();
			} else {
				showSection("auth");
				refreshUserArea();
				NomadAuth.setTab("login");
			}
		});
	}

	// bindings
	// Call enterGallery() directly after login/register — no need to re-validate
	// the token via me() since we just received it. Calling init() here would add
	// an unnecessary round-trip and could silently log the user out on any transient
	// network error, leaving the resume/dismiss handlers never set.
	NomadAuth.bind(function () { enterGallery(); });
	$("logout-btn").addEventListener("click", function () { NomadAPI.logout(); init(); });
	$("close-btn").addEventListener("click", closeGallery);
	$("refresh-btn").addEventListener("click", function () { renderGrid(); });
	$("save-new-btn").addEventListener("click", saveCurrentAsNew);
	$("import-btn").addEventListener("click", function () { $("import-file").click(); });
	$("import-file").addEventListener("change", function (e) {
		importFromDevice(e.target.files && e.target.files[0]);
		e.target.value = ""; // allow re-importing the same file
	});

	// Parent asks us to refresh when the overlay is (re)opened.
	window.addEventListener("message", function (e) {
		if (e.origin !== window.location.origin || !e.data) { return; }
		if (e.data.type === "nomad:open" && NomadAPI.isLoggedIn()) {
			refreshUserArea();
			renderGrid().then(maybeResume);
		}
	});

	init();
})();
