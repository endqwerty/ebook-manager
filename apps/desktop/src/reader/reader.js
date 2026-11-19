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
	const width = () => root.clientWidth || innerWidth;
	const pages = () => Math.max(1, Math.round(root.scrollWidth / width()));
	const page = () => Math.round(Math.abs(root.scrollLeft) / width());
	const show = (n, smooth) => {
		const target = Math.max(0, Math.min(n, pages() - 1)) * width() * (rtl() ? -1 : 1);
		root.classList.toggle("slide", !!smooth);
		root.scrollLeft = target;
		if (!smooth) root.classList.remove("slide");
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
	const report = (chosen) => {
		if (!ready || !landed) return;
		const offset = place();
		if (chosen) {
			const last = page() === pages() - 1;
			const edge = !textless ? null : last && chosen === "end" ? "end" : page() === 0 ? "start" : last ? "end" : null;
			held = { offset, edge, textless, page: page(), pages: pages() };
		}
		post({ kind: "place", load, offset, page: page(), pages: pages() });
	};

	// Show the held place again after the layout changed.
	const restore = () => {
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
			} else if (to.page !== undefined) {
				show(to.page);
			} else {
				show(pageOfOffset(to.offset || 0));
			}
			landed = true;
			report(to.end ? "end" : true);
			return true;
		},
		// Turn by `by` pages. False at either edge of the chapter.
		turn(by, smooth) {
			const next = page() + by;
			if (next < 0 || next >= pages()) return false;
			show(next, smooth);
			report(true);
			return true;
		},
		// Set custom properties with no reflow handling, before a landing.
		assign(properties) {
			assign(properties);
			return true;
		},
		// Set custom properties, keeping the place. A new face can change
		// the layout when it arrives, so the place is restored after that.
		style(properties) {
			assign(properties);
			const after = () => settle(restore);
			document.fonts.ready.then(after, after);
			return true;
		},
		// The character at an offset, for checking the count against the helper's.
		charAt(offset) {
			const at = locate(offset);
			return at ? at[0].data.charAt(at[1]) : "";
		},
		length: () => length,
		// The place showing now, as "offset,page,pages", or "" before the
		// landing. A turn asked for earlier has happened by the time this
		// answers, because calls are answered in the order they were made.
		where: () => (ready && landed ? [place(), page(), pages()].join(",") : ""),
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
			const along = (rtl() ? width() - event.clientX : event.clientX) / width();
			const zone = along < 0.25 ? "back" : along >= 0.75 ? "forward" : along >= 1 / 3 && along < 2 / 3 ? "middle" : "";
			if (zone) post({ kind: "tap", load, zone });
		},
		true,
	);
	document.addEventListener("keydown", (event) => {
		const link = event.target instanceof Element ? event.target.closest("a[data-external]") : null;
		if (link && event.key === "Enter") link.click();
	});

	let resizing = 0;
	addEventListener("resize", () => {
		if (!ready) return;
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
		settle(() => {
			ready = true;
			post({ kind: "ready", load, length, pages: pages(), body: true });
		});
	};
	if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
	else start();
})();
