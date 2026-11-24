// The app's script for a chapter. It runs in its own script world: the page
// has no access to it, and it never evaluates anything the page holds.
(() => {
	"use strict";
	const post = (m) => window.webkit.messageHandlers.reader.postMessage(m);
	const load = Number(new URLSearchParams(location.search).get("l")) || 0;
	const root = document.documentElement;
	let nodes = [];
	let starts = [];
	let length = 0;
	let ready = false;
	// The shell has named the landing place. Nothing is reported before that:
	// a resize in between would report the first page as the reader's place.
	let landed = false;
	// The layout has changed and the held place has not been shown again yet.
	let reflowing = false;
	let held = { offset: 0, edge: "start", textless: false, page: 0, pages: 1 };

	const index = () => {
		nodes = [];
		starts = [];
		length = 0;
		if (!document.body) return;
		const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
		while (walker.nextNode()) {
			nodes.push(walker.currentNode);
			starts.push(length);
			length += walker.currentNode.data.length;
		}
	};

	// The text node holding `offset` and the offset inside it.
	const locate = (offset) => {
		if (!nodes.length) return null;
		const at = Math.max(0, Math.min(offset, length));
		let lo = 0;
		let hi = nodes.length - 1;
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1;
			if (starts[mid] <= at) lo = mid;
			else hi = mid - 1;
		}
		return [nodes[lo], Math.min(at - starts[lo], nodes[lo].data.length)];
	};

	const offsetOf = (node, inner) => {
		let lo = 0;
		let hi = nodes.length - 1;
		// Text nodes are in document order, so compare positions.
		while (lo <= hi) {
			const mid = (lo + hi) >> 1;
			if (nodes[mid] === node) return starts[mid] + inner;
			const before = nodes[mid].compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING;
			if (before) lo = mid + 1;
			else hi = mid - 1;
		}
		return null;
	};

	const rtl = () => getComputedStyle(root).direction === "rtl";
	const setting = (name) => getComputedStyle(root).getPropertyValue(name).trim();
	const width = () => root.clientWidth || innerWidth;
	const pages = () => Math.max(1, Math.round(root.scrollWidth / width()));
	const page = () => Math.round(Math.abs(root.scrollLeft) / width());
	const at = () => root.scrollLeft;
	// Where a slide or a fade that has not finished is going, or null. A
	// slide keeps it until the text reaches it, and an instant move
	// replaces it. No timer clears it, so a slow slide still counts as
	// going where it is going.
	let aim = null;
	// The fade's second half, until it runs.
	let fading = 0;
	// End a fade that has not reached its second half where the text is,
	// and bring the page back in.
	const stopFade = () => {
		clearTimeout(fading);
		fading = 0;
		root.classList.remove("fading");
	};
	// Scroll to `x` at once, or as a slide. Going anywhere at once ends a
	// slide or a fade that has not finished.
	const goTo = (x, smooth) => {
		stopFade();
		// A slide to where the text already is has nothing to wait for.
		const slide = smooth && Math.abs(at() - x) >= 1;
		root.classList.toggle("slide", slide);
		root.scrollLeft = x;
		if (!slide) root.classList.remove("slide");
		aim = slide ? x : null;
	};
	// Fade the page out, go to `x`, and fade it back in.
	const fadeTo = (x) => {
		stopFade();
		root.classList.add("fade", "fading");
		aim = x;
		fading = setTimeout(() => goTo(x, false), 120);
	};
	// The layout is changing: an unfinished slide or fade and where it was
	// going belong to the old layout, so they end here. Only `restore`, and
	// the two places that set `reflowing` and so have a `restore` coming,
	// call this, and `restore` shows the held place, which is where the
	// turn led, and tells it.
	const forget = () => {
		stopFade();
		root.classList.remove("slide");
		aim = null;
	};
	// Show page `n` at once.
	const show = (n) => {
		const to = Math.max(0, Math.min(n, pages() - 1));
		goTo(to * width() * (rtl() ? -1 : 1), false);
	};

	// Apply the setting that changes the document's shape: two columns
	// when asked for and the window fits two of 60 characters.
	const layout = () => {
		const em = parseFloat(getComputedStyle(root).fontSize) || 16;
		const edge = 48 * (parseFloat(setting("--margin")) || 1);
		const fits = (width() - 4 * edge) / 2 >= 30 * em;
		root.classList.toggle("two", setting("--columns") === "2" && fits);
		syncVisuals();
	};

	// #129 Dark-page image classification (runs once per image when Match the page is active).
	const classifyImage = (img) => {
		if (img.dataset.lineartChecked === "1") return;
		img.dataset.lineartChecked = "1";
		if (!img.complete || img.naturalWidth === 0 || img.naturalHeight === 0) return;
		try {
			const c = document.createElementNS("http://www.w3.org/1999/xhtml", "canvas");
			const w = Math.min(32, img.naturalWidth);
			const h = Math.min(32, img.naturalHeight);
			c.width = w;
			c.height = h;
			const ctx = c.getContext("2d");
			if (!ctx) return;
			ctx.drawImage(img, 0, 0, w, h);
			const data = ctx.getImageData(0, 0, w, h).data;
			let opaque = 0;
			let lowChroma = 0;
			let lightPixels = 0;
			let transparent = 0;
			for (let i = 0; i < data.length; i += 4) {
				const a = data[i + 3];
				if (a < 32) {
					transparent += 1;
					continue;
				}
				opaque += 1;
				const r = data[i], g = data[i + 1], b = data[i + 2];
				const spread = Math.max(r, g, b) - Math.min(r, g, b);
				if (spread < 28) lowChroma += 1;
				if ((0.2126 * r + 0.7152 * g + 0.0722 * b) > 165) lightPixels += 1;
			}
			const total = w * h;
			if (opaque > 0 && lowChroma / opaque >= 0.94 && (lightPixels / opaque >= 0.55 || transparent / total >= 0.25)) {
				img.dataset.lineart = "1";
			}
		} catch (_) {}
	};

	// #134 Reading ruler / line focus overlays live on `root`, outside `body`,
	// so `body` text offsets and `:first-child` rules stay untouched.
	let rulerTop = null;
	let rulerBottom = null;
	let rulerOutline = null;
	let rulerLine = 0;

	const ensureRuler = () => {
		if (rulerTop) return;
		const make = (cls) => {
			const el = document.createElementNS("http://www.w3.org/1999/xhtml", "div");
			el.className = cls;
			el.setAttribute("aria-hidden", "true");
			root.appendChild(el);
			return el;
		};
		rulerTop = make("reader-ruler-mask");
		rulerBottom = make("reader-ruler-mask");
		rulerOutline = make("reader-ruler-outline");
	};

	const rulerMetrics = () => {
		const style = document.body ? getComputedStyle(document.body) : getComputedStyle(root);
		const padTop = parseFloat(style.paddingTop) || 0;
		const padBottom = parseFloat(style.paddingBottom) || 0;
		const fontSize = parseFloat(style.fontSize) || 18;
		const parsedLh = parseFloat(style.lineHeight);
		const lh = Number.isFinite(parsedLh) && parsedLh > 0 ? parsedLh : fontSize * 1.5;
		const h = root.clientHeight || innerHeight;
		const contentH = Math.max(lh, h - padTop - padBottom);
		const bandLines = parseInt(setting("--line-focus"), 10) || 0;
		const totalLines = Math.max(1, Math.floor(contentH / lh));
		return { padTop, h, lh, bandLines, totalLines };
	};

	const renderRuler = () => {
		const { padTop, h, lh, bandLines, totalLines } = rulerMetrics();
		if (bandLines <= 0) {
			if (rulerTop) {
				rulerTop.style.display = "none";
				rulerBottom.style.display = "none";
				rulerOutline.style.display = "none";
			}
			return;
		}
		ensureRuler();
		const maxStart = Math.max(0, totalLines - bandLines);
		rulerLine = Math.max(0, Math.min(rulerLine, maxStart));
		const top = Math.max(0, padTop + rulerLine * lh - 2);
		const bandH = Math.min(h - top, bandLines * lh + 4);
		const dim = setting("--focus-dim");
		if (dim === "contrast") {
			rulerTop.style.display = "none";
			rulerBottom.style.display = "none";
			rulerOutline.style.display = "block";
			rulerOutline.style.top = `${top}px`;
			rulerOutline.style.height = `${bandH}px`;
		} else {
			const alpha = parseFloat(dim) || 0.55;
			rulerOutline.style.display = "none";
			rulerTop.style.display = "block";
			rulerTop.style.opacity = String(alpha);
			rulerTop.style.top = "0px";
			rulerTop.style.height = `${top}px`;
			rulerBottom.style.display = "block";
			rulerBottom.style.opacity = String(alpha);
			rulerBottom.style.top = `${top + bandH}px`;
			rulerBottom.style.height = `${Math.max(0, h - (top + bandH))}px`;
		}
	};

	const syncVisuals = () => {
		const mode = setting("--dark-images") || "original";
		root.dataset.darkImages = mode;
		root.dataset.motion = setting("--motion") || "full";
		if (mode === "match") {
			for (const img of document.images) classifyImage(img);
		}
		renderRuler();
	};

	// The page a point of the document falls on, from its place on screen now.
	const pageOfX = (x) => {
		// After a resize the scroll offset is not yet a whole number of pages.
		const along = rtl() ? width() - x : x;
		return Math.floor((Math.abs(root.scrollLeft) + along) / width());
	};

	const pageOfOffset = (offset) => {
		// The end of the text stands for a last page with no text on it.
		if (offset >= length) return pages() - 1;
		const at = locate(offset);
		if (!at) return 0;
		const range = document.createRange();
		const [node, inner] = at;
		if (inner < node.data.length) {
			range.setStart(node, inner);
			range.setEnd(node, inner + 1);
		} else {
			range.selectNodeContents(node);
			range.collapse(false);
		}
		const rects = range.getClientRects();
		const rect = rects.length ? rects[0] : range.getBoundingClientRect();
		return pageOfX(rect.left + 1);
	};

	// Whether the page showing has any text on it. Set by `place`.
	let textless = false;

	// The offset of the first character on the page showing.
	const place = () => {
		textless = true;
		if (!nodes.length) return 0;
		const style = getComputedStyle(document.body);
		const side = parseFloat(style.paddingLeft) || 0;
		const top = parseFloat(style.paddingTop) || 0;
		const w = width();
		const h = root.clientHeight || innerHeight;
		for (let y = top + 2; y < h - top; y += 12) {
			for (const x of rtl() ? [w - side - 2, w / 2] : [side + 2, w / 2]) {
				const range = document.caretRangeFromPoint(x, y);
				if (range && range.startContainer.nodeType === Node.TEXT_NODE) {
					const offset = offsetOf(range.startContainer, range.startOffset);
					if (offset !== null) {
						textless = false;
						return offset;
					}
				}
			}
		}
		// A page with no text, such as one picture: the first text after it.
		const here = page();
		let lo = 0;
		let hi = nodes.length;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (pageOfOffset(starts[mid]) < here) lo = mid + 1;
			else hi = mid;
		}
		return lo < nodes.length ? starts[lo] : length;
	};

	// `held` is the place the reader chose, by a turn or a jump. A reflow
	// goes back to it and does not move it, so resizing a window through many
	// widths does not creep toward the chapter's start.
	//
	// A page with no text has no offset of its own. At either end of the
	// chapter it is held as that end, so a reflow keeps a chapter on its
	// picture-only first or last page. In the middle it is held as the first
	// text after it, which is all an offset can say.
	// `chosen` is "end" when the reader asked for the chapter's end, which
	// decides it on a one-page chapter where both ends are the same page.
	//
	// The offset reported is always the held place's: the one a turn or a
	// jump chose. A reflow changes the page count and the page number, and
	// not the place to save, so a resize saves nothing new.
	//
	// `moves` counts the moves made in this chapter. With the load's number
	// it orders every place the shell is told, so a late answer can never
	// replace a later place. A report that is not a move repeats the count
	// of the move it follows.
	let moves = 0;
	const settle_place = (chosen) => {
		if (chosen) {
			moves += 1;
			const offset = place();
			const last = page() === pages() - 1;
			const edge = !textless ? null : last && chosen === "end" ? "end" : page() === 0 ? "start" : last ? "end" : null;
			held = { offset, edge, textless, page: page(), pages: pages() };
		}
		return { offset: held.offset, page: page(), pages: pages(), move: moves, moved: !!chosen };
	};
	const report = (chosen) => {
		if (!ready || !landed) return;
		post({ kind: "place", load, ...settle_place(chosen) });
	};

	// Show the held place again after the layout changed.
	const restore = () => {
		reflowing = false;
		forget();
		layout();
		if (!landed) return;
		if (held.edge === "start") show(0);
		else if (held.edge === "end") show(pages() - 1);
		// A text-less page in the middle keeps its number while the chapter
		// keeps its page count. Otherwise only the text after it can be found.
		else if (held.textless && held.pages === pages()) show(held.page);
		else show(pageOfOffset(held.offset));
		report();
	};

	const assign = (properties) => {
		for (const [name, value] of Object.entries(properties)) {
			if (/^--[a-z-]+$/.test(name)) root.style.setProperty(name, String(value));
		}
	};

	const settle = (then) => requestAnimationFrame(() => requestAnimationFrame(then));

	const api = {
		// Show the page holding a place. `to` is {offset}, {anchor}, {end} or {page}.
		go(to) {
			if (to.anchor !== undefined) {
				const element = document.getElementById(to.anchor);
				if (element) {
					const rect = element.getBoundingClientRect();
					show(pageOfX(rect.left + 1));
				} else {
					show(0);
				}
			} else if (to.end) {
				show(pages() - 1);
				const { bandLines, totalLines } = rulerMetrics();
				if (bandLines > 0) {
					rulerLine = Math.max(0, totalLines - bandLines);
					renderRuler();
				}
			} else if (to.page !== undefined) {
				show(to.page);
			} else {
				show(pageOfOffset(to.offset || 0));
			}
			landed = true;
			report(to.end ? "end" : true);
			return true;
		},
		// Turn by `by` pages. `smooth` uses the reader's page turn: a slide,
		// a fade or none, and none when the system asks for less motion.
		//
		// The answer is the new place, so the shell learns of the turn and
		// where it led in one step. "" means the chapter's edge. The place is
		// read at the destination before any animation starts: the text goes
		// there, the place is read, and only then does it go back and slide
		// or fade. All of that is one task, so nothing is drawn in between.
		// A turn made while another is still animating starts from where
		// that one is going.
		turn(by, smooth) {
			if (!ready || !landed) return "";
			// The layout changed and the held place is not shown in it yet:
			// show it first, so the turn counts from it.
			if (reflowing) restore();
			const before = at();
			const next = (aim === null ? page() : Math.round(Math.abs(aim) / width())) + by;
			if (next < 0 || next >= pages()) return "";
			const target = next * width() * (rtl() ? -1 : 1);
			goTo(target, false);
			const now = settle_place(true);
			// The shell says when the system asks for less motion. WebKitGTK
			// does not tell the page itself.
			const motion = smooth && setting("--motion") !== "none" && matchMedia("(prefers-reduced-motion: no-preference)").matches;
			const kind = motion ? setting("--turn") : "none";
			if (kind === "slide" || kind === "fade") {
				goTo(before, false);
				if (kind === "slide") goTo(target, true);
				else fadeTo(target);
			}
			return [now.offset, now.page, now.pages, now.move].join(",");
		},
		// Set custom properties with no reflow handling, before a landing.
		assign(properties) {
			assign(properties);
			layout();
			return true;
		},
		// Set custom properties, keeping the place. A new face can change
		// the layout when it arrives, so the place is restored after that.
		style(properties) {
			assign(properties);
			reflowing = true;
			forget();
			const after = () => settle(restore);
			document.fonts.ready.then(after, after);
			return true;
		},
		// The character at an offset, for checking the count against the helper's.
		charAt(offset) {
			const at = locate(offset);
			return at ? at[0].data.charAt(at[1]) : "";
		},
		stepRuler(by) {
			const { bandLines, totalLines } = rulerMetrics();
			if (bandLines <= 0) return "off";
			const maxStart = Math.max(0, totalLines - bandLines);
			const next = rulerLine + by;
			if (next < 0 || next > maxStart) return "edge";
			rulerLine = next;
			renderRuler();
			return "moved";
		},
		resetRuler(edge) {
			const { bandLines, totalLines } = rulerMetrics();
			if (bandLines <= 0) return false;
			rulerLine = edge === "end" ? Math.max(0, totalLines - bandLines) : 0;
			renderRuler();
			return true;
		},
		length: () => length,
	};
	Object.defineProperty(window, "__reader", { value: Object.freeze(api) });

	document.addEventListener(
		"click",
		(event) => {
			const link = event.target instanceof Element ? event.target.closest("a") : null;
			if (link) {
				event.preventDefault();
				const external = link.getAttribute("data-external");
				const href = link.getAttribute("href");
				if (external) post({ kind: "external", load, address: external });
				else if (href) post({ kind: "jump", load, href });
				return;
			}
			if (!getSelection().isCollapsed) return;
			const { padTop, lh, bandLines, totalLines } = rulerMetrics();
			if (bandLines > 0) {
				const line = Math.floor((event.clientY - padTop) / lh);
				if (line >= 0 && line < totalLines) {
					rulerLine = Math.max(0, Math.min(line, Math.max(0, totalLines - bandLines)));
					renderRuler();
					return;
				}
			}
			const along = (rtl() ? width() - event.clientX : event.clientX) / width();
			const zone = along < 0.25 ? "back" : along >= 0.75 ? "forward" : along >= 1 / 3 && along < 2 / 3 ? "middle" : "";
			if (zone) post({ kind: "tap", load, zone });
		},
		true,
	);
	// Double-clicking an image toggles Show original (#129).
	document.addEventListener(
		"dblclick",
		(event) => {
			const img = event.target instanceof Element ? event.target.closest("img") : null;
			if (!img) return;
			if (img.dataset.original === "1") delete img.dataset.original;
			else img.dataset.original = "1";
		},
		true,
	);
	document.addEventListener("keydown", (event) => {
		const link = event.target instanceof Element ? event.target.closest("a[data-external]") : null;
		if (link && event.key === "Enter") link.click();
	});

	// A slide has ended when the text reaches its end.
	addEventListener(
		"scroll",
		() => {
			if (aim !== null && !fading && Math.abs(at() - aim) < 1) aim = null;
		},
		true,
	);

	let resizing = 0;
	addEventListener("resize", () => {
		if (!ready) return;
		reflowing = true;
		forget();
		cancelAnimationFrame(resizing);
		resizing = requestAnimationFrame(restore);
	});

	const start = async () => {
		if (!document.body) {
			post({ kind: "ready", load, length: 0, pages: 0, body: false });
			return;
		}
		for (const link of document.querySelectorAll("a[data-external]")) {
			link.setAttribute("tabindex", "0");
			link.setAttribute("role", "link");
		}
		try {
			await document.fonts.ready;
		} catch (_) {}
		await Promise.all(
			Array.from(document.images, (image) =>
				image.complete ? null : new Promise((done) => {
					image.addEventListener("load", done, { once: true });
					image.addEventListener("error", done, { once: true });
				}),
			),
		);
		index();
		layout();
		settle(() => {
			ready = true;
			post({ kind: "ready", load, length, pages: pages(), body: true });
		});
	};
	if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
	else start();
})();
