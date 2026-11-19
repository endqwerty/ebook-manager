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
	// Continuous scrolling instead of pages. Set by `layout`.
	let scrolling = false;
	const width = () => root.clientWidth || innerWidth;
	const height = () => root.clientHeight || innerHeight;
	// In scrolling a "page" is a screenful, so progress and turning still work.
	const pages = () =>
		scrolling
			? Math.max(1, Math.ceil((root.scrollHeight - 1) / height()))
			: Math.max(1, Math.round(root.scrollWidth / width()));
	const page = () =>
		scrolling
			? Math.min(pages() - 1, Math.round(root.scrollTop / height()))
			: Math.round(Math.abs(root.scrollLeft) / width());
	// The scroll position along the way the text moves.
	const at = () => (scrolling ? root.scrollTop : root.scrollLeft);
	// Where a slide or a fade that has not finished is going, or null.
	let aim = null;
	// Where the app last put the text. A scroll that settles anywhere else
	// was the reader's own.
	let expect = 0;
	// The fade's second half, until it runs.
	let fading = 0;
	// Scroll to `x` at once, or as a slide. Going anywhere at once ends a
	// slide or a fade that has not finished.
	const goTo = (x, smooth) => {
		clearTimeout(fading);
		root.classList.remove("fading");
		root.classList.toggle("slide", !!smooth);
		if (scrolling) root.scrollTop = x;
		else root.scrollLeft = x;
		if (!smooth) root.classList.remove("slide");
		aim = smooth ? x : null;
		expect = smooth ? x : at();
	};
	// Fade the page out, go to `x`, and fade it back in.
	const fadeTo = (x) => {
		clearTimeout(fading);
		root.classList.add("fade", "fading");
		aim = x;
		expect = x;
		fading = setTimeout(() => goTo(x, false), 120);
	};
	// Show page `n` at once.
	const show = (n) => {
		const to = Math.max(0, Math.min(n, pages() - 1));
		if (scrolling) goTo(Math.min(to * height(), Math.max(0, root.scrollHeight - height())), false);
		else goTo(to * width() * (rtl() ? -1 : 1), false);
	};

	// Apply the settings that change the document's shape: two columns when
	// asked for and the window fits two of 60 characters, and scrolling.
	const layout = () => {
		scrolling = setting("--flow") === "scroll";
		root.classList.toggle("scroll", scrolling);
		const em = parseFloat(getComputedStyle(root).fontSize) || 16;
		const edge = 48 * (parseFloat(setting("--margin")) || 1);
		const fits = (width() - 4 * edge) / 2 >= 30 * em;
		root.classList.toggle("two", !scrolling && setting("--columns") === "2" && fits);
	};

	// The page a point of the document falls on, from its place on screen now.
	const pageOfX = (x, y) => {
		if (scrolling) return Math.floor((root.scrollTop + y + 1) / height());
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
		return pageOfX(rect.left + 1, rect.top);
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
		const h = height();
		// In scrolling the top margin has scrolled away with the page.
		const first = scrolling ? 2 : top + 2;
		const last = scrolling ? h : h - top;
		for (let y = first; y < last; y += 12) {
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

	// Show the page holding an offset. In scrolling the offset is brought to
	// the top of the screen, where a place is read from.
	const showOffset = (offset) => {
		show(pageOfOffset(offset));
		const at = scrolling && offset > 0 && offset < length ? locate(offset) : null;
		if (at) {
			const range = document.createRange();
			range.setStart(at[0], at[1]);
			range.collapse(true);
			goTo(root.scrollTop + range.getBoundingClientRect().top, false);
		}
	};

	// Show the held place again after the layout changed.
	const restore = () => {
		reflowing = false;
		layout();
		if (!landed) return;
		if (held.edge === "start") show(0);
		else if (held.edge === "end") show(pages() - 1);
		// A text-less page in the middle keeps its number while the chapter
		// keeps its page count. Otherwise only the text after it can be found.
		else if (held.textless && held.pages === pages()) show(held.page);
		else showOffset(held.offset);
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
					show(pageOfX(rect.left + 1, rect.top));
				} else {
					show(0);
				}
			} else if (to.end) {
				show(pages() - 1);
			} else if (to.page !== undefined) {
				show(to.page);
			} else {
				showOffset(to.offset || 0);
			}
			landed = true;
			report(to.end ? "end" : true);
			return true;
		},
		// Turn by `by` pages, or in scrolling by a screenful less a line, so
		// the eye keeps its place. `smooth` uses the reader's page turn: a
		// slide, a fade or none, and none when the system asks for less
		// motion.
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
			const before = at();
			let target;
			if (scrolling) {
				const from = aim === null ? before : aim;
				const line = parseFloat(getComputedStyle(root).lineHeight) || 24;
				const most = Math.max(0, root.scrollHeight - height());
				target = Math.max(0, Math.min(most, from + by * Math.max(40, height() - line)));
				if (Math.abs(target - from) < 1) return "";
			} else {
				const next = (aim === null ? page() : Math.round(Math.abs(aim) / width())) + by;
				if (next < 0 || next >= pages()) return "";
				target = next * width() * (rtl() ? -1 : 1);
			}
			goTo(target, false);
			const now = settle_place(true);
			const motion = smooth && matchMedia("(prefers-reduced-motion: no-preference)").matches;
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

	// Scrolling settles 150 ms after its last step. A slide has finished by
	// then. In scrolling, a scroll by wheel, keys or scroll bar is a move,
	// reported where it settles. Where the app put the text was reported
	// when it was put there, and a reflow keeps the held place.
	let scrolled = 0;
	const settled = () => {
		aim = null;
		if (!scrolling || !ready || !landed || reflowing) return;
		if (Math.abs(at() - expect) < 1) return;
		expect = at();
		report(true);
	};
	addEventListener(
		"scroll",
		() => {
			clearTimeout(scrolled);
			scrolled = setTimeout(settled, 150);
		},
		true,
	);

	let resizing = 0;
	addEventListener("resize", () => {
		if (!ready) return;
		reflowing = true;
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
