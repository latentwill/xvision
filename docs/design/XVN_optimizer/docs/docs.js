/* xvn docs — minimal, optional client-side enhancements.
   All content is readable without JS. JS adds: theme toggle, copy-as-md,
   TOC scrollspy, command-K search hint, code-block copy. */

(function () {
  const root = document.documentElement;

  // Persisted prefs
  const PREFS = {
    theme: 'dark',
    density: 'compact',
    toc: 'shown',
    context: 'shown',
  };
  try {
    const saved = JSON.parse(localStorage.getItem('xvn-docs-prefs') || '{}');
    Object.assign(PREFS, saved);
  } catch (_) {}
  applyPrefs();

  function applyPrefs() {
    root.setAttribute('data-theme', PREFS.theme);
    root.setAttribute('data-density', PREFS.density);
    root.setAttribute('data-toc', PREFS.toc);
    root.setAttribute('data-context', PREFS.context);
  }
  function savePrefs() {
    try { localStorage.setItem('xvn-docs-prefs', JSON.stringify(PREFS)); } catch (_) {}
  }
  function setPref(k, v) {
    PREFS[k] = v;
    applyPrefs();
    savePrefs();
    syncTweaks();
  }

  // ===== Theme toggle button (topbar) =====
  document.addEventListener('click', (e) => {
    const t = e.target.closest('[data-action]');
    if (!t) return;
    const action = t.dataset.action;
    if (action === 'toggle-theme') {
      setPref('theme', PREFS.theme === 'dark' ? 'light' : 'dark');
    } else if (action === 'open-tweaks') {
      document.querySelector('.tweaks')?.classList.add('open');
    } else if (action === 'close-tweaks') {
      document.querySelector('.tweaks')?.classList.remove('open');
    } else if (action === 'copy-md') {
      copyMd(t);
    } else if (action === 'copy-code') {
      copyCode(t);
    } else if (action?.startsWith('set:')) {
      const [, k, v] = action.split(':');
      setPref(k, v);
    }
  });

  // ===== Sync Tweaks panel UI =====
  function syncTweaks() {
    document.querySelectorAll('.tweaks .seg').forEach((seg) => {
      const k = seg.dataset.key;
      seg.querySelectorAll('button').forEach((b) => {
        b.classList.toggle('on', b.dataset.value === PREFS[k]);
      });
    });
  }
  syncTweaks();

  // ===== Copy code blocks =====
  function copyCode(btn) {
    const pre = btn.closest('.code')?.querySelector('pre');
    if (!pre) return;
    navigator.clipboard.writeText(pre.innerText).then(() => {
      const orig = btn.textContent;
      btn.textContent = 'Copied';
      btn.classList.add('copied');
      setTimeout(() => { btn.textContent = orig; btn.classList.remove('copied'); }, 1400);
    });
  }

  // ===== Copy as Markdown =====
  function copyMd(btn) {
    const url = btn.dataset.mdUrl;
    if (!url) return;
    btn.classList.add('copied');
    const label = btn.querySelector('span');
    const prev = label?.textContent;
    if (label) label.textContent = 'Fetching…';
    fetch(url)
      .then((r) => r.ok ? r.text() : Promise.reject(r.status))
      .then((md) => navigator.clipboard.writeText(md))
      .then(() => { if (label) label.textContent = 'Copied as Markdown'; })
      .catch(() => { if (label) label.textContent = `See ${url}`; })
      .finally(() => {
        setTimeout(() => {
          btn.classList.remove('copied');
          if (label && prev) label.textContent = prev;
        }, 1800);
      });
  }

  // ===== Scrollspy: TOC links + sidebar same-page anchors =====
  const here = location.pathname.replace(/\/$/, '');
  function samePage(href) {
    if (!href) return false;
    try {
      const u = new URL(href, location.href);
      return u.pathname.replace(/\/$/, '') === here;
    } catch (_) { return false; }
  }

  const tocLinks = Array.from(document.querySelectorAll('.toc a[href*="#"]'));
  // Sidebar links that point to a section on the CURRENT page
  const sideAnchorLinks = Array.from(document.querySelectorAll('.sidebar a[href*="#"]'))
    .filter((a) => samePage(a.getAttribute('href')));
  // Sidebar links that point to the page itself (no hash). Used as the "default" active link
  // when no tracked section is in view yet.
  const sidePageLinks = Array.from(document.querySelectorAll('.sidebar a'))
    .filter((a) => {
      const href = a.getAttribute('href') || '';
      return samePage(href) && !href.includes('#');
    });

  const allTracked = [...tocLinks, ...sideAnchorLinks];
  if (allTracked.length) {
    const idSet = new Set();
    allTracked.forEach((a) => {
      const id = (a.getAttribute('href') || '').split('#')[1];
      if (id) idSet.add(id);
    });
    const targets = Array.from(idSet)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .sort((a, b) => a.offsetTop - b.offsetTop);
    const linksById = new Map();
    allTracked.forEach((a) => {
      const id = (a.getAttribute('href') || '').split('#')[1];
      if (!id) return;
      if (!linksById.has(id)) linksById.set(id, []);
      linksById.get(id).push(a);
    });

    let activeId = null;
    function clearActive() {
      allTracked.forEach((a) => a.classList.remove('active'));
    }
    function setActive(id) {
      if (id === activeId) return;
      activeId = id;
      clearActive();
      if (id && linksById.has(id)) {
        linksById.get(id).forEach((a) => a.classList.add('active'));
        // A sub-section is in view — the page-level link steps back.
        sidePageLinks.forEach((p) => p.classList.remove('active'));
      } else {
        // No tracked section in view — page-level link is the default active.
        sidePageLinks.forEach((p) => p.classList.add('active'));
      }
    }

    function recompute() {
      // Find the topmost heading whose top has scrolled into the "active" zone.
      const offset = 96;
      let current = null;
      for (const t of targets) {
        const r = t.getBoundingClientRect();
        if (r.top - offset <= 0) current = t.id; else break;
      }
      // Edge case: at/near page bottom, pin to the last section so the trailing
      // heading stays highlighted even when it can't scroll above the offset.
      const atBottom = window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 4;
      if (atBottom && targets.length) current = targets[targets.length - 1].id;
      setActive(current);
    }
    recompute();
    window.addEventListener('scroll', recompute, { passive: true });
    window.addEventListener('resize', recompute);
  }

  // ===== Keyboard: ⌘K / Ctrl+K focuses the search hint (mock) =====
  document.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      const s = document.querySelector('.search');
      if (s) {
        s.style.outline = '1px solid var(--accent)';
        setTimeout(() => { s.style.outline = ''; }, 700);
      }
    }
  });
})();
