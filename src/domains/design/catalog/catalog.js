
(function () {
  var root = document.documentElement;
  var rails = document.getElementById('ds-rails');
  var items = Array.prototype.slice.call(document.querySelectorAll('.ds-item'));
  var subs = Array.prototype.slice.call(document.querySelectorAll('.ds-subitem'));

  var main = document.querySelector('.ds-main');
  var flow = document.getElementById('ds-flow');
  var pages = Array.prototype.slice.call(document.querySelectorAll('.ds-page'));
  var cats = Array.prototype.slice.call(flow.querySelectorAll('.ds-cat'));

  function mark(id) {
    items.forEach(function (b) {
      if (b.dataset.cat === id) { b.setAttribute('aria-current', 'page'); }
      else { b.removeAttribute('aria-current'); }
    });
  }
  function page(id) {
    flow.hidden = true;
    pages.forEach(function (p) { p.hidden = p.dataset.page !== id; });
    mark(id);
    main.scrollTop = 0;
  }
  function select(id) {
    var smooth = !flow.hidden;
    flow.hidden = false;
    pages.forEach(function (p) { p.hidden = true; });
    var target = document.getElementById('cat-' + id);
    if (target) { main.scrollTo({ top: target.offsetTop, behavior: smooth ? 'smooth' : 'auto' }); }
    mark(id);
  }
  function spy() {
    if (flow.hidden) { return; }
    var line = main.scrollTop + 80;
    var live = cats.filter(function (s) { return !s.classList.contains('ds-miss'); });
    if (!live.length) { return mark(''); }
    var current = live[0];
    live.forEach(function (s) { if (s.offsetTop <= line) { current = s; } });
    if (main.scrollTop + main.clientHeight >= main.scrollHeight - 2) { current = live[live.length - 1]; }
    mark(current.dataset.cat);
  }
  function open(drill) { rails.classList.toggle('ds-open', !!drill); }

  main.addEventListener('scroll', spy, { passive: true });
  items.forEach(function (b) {
    b.addEventListener('click', function () {
      var id = b.dataset.cat;
      if (pages.some(function (p) { return p.dataset.page === id; })) { page(id); } else { select(id); }
      open(b.dataset.drill === '1');
    });
  });
  document.querySelectorAll('[data-open]').forEach(function (c) {
    c.addEventListener('click', function () { page(c.dataset.open); open(c.dataset.open === 'components'); });
  });
  var back = document.querySelector('.ds-back');
  if (back) { back.addEventListener('click', function () { open(false); select('components'); }); }
  subs.forEach(function (a) {
    a.addEventListener('click', function () { page('components'); open(true); });
  });
  spy();

  var q = document.getElementById('ds-q');
  var empty = document.getElementById('ds-empty');
  var entries = '.ds-row, .ds-trow, .ds-specimen, .ds-comp';
  function has(el, v) { return el.textContent.toLowerCase().indexOf(v) >= 0; }
  function shown(s) { return s.querySelector(visible); }
  var visible = entries.split(', ').map(function (e) { return e + ':not([hidden])'; }).join(', ');
  function filter() {
    var v = q.value.trim().toLowerCase();
    var any = false;
    cats.concat(pages).forEach(function (s) {
      var id = s.dataset.cat || s.dataset.page;
      var label = items.filter(function (b) { return b.dataset.cat === id; })[0];
      var all = v === '' || (label && has(label, v));
      var rows = s.querySelectorAll(entries);
      rows.forEach(function (e) { e.hidden = !all && !has(e, v); });
      s.querySelectorAll('.ds-tgroup').forEach(function (g) { g.hidden = !all && !shown(g); });
      if (!s.dataset.cat) { return; }
      var hit = all || (rows.length ? !!shown(s) : has(s, v));
      s.classList.toggle('ds-miss', !hit);
      if (label) { label.hidden = !hit; }
      any = any || hit;
    });
    subs.forEach(function (a) { a.hidden = v !== '' && (a.dataset.name || '').indexOf(v) < 0; });
    var rule = document.querySelector('.ds-rule');
    if (rule) { rule.hidden = !!(rule.nextElementSibling && rule.nextElementSibling.hidden); }
    empty.hidden = any;
    spy();
  }
  q.addEventListener('input', filter);
  q.addEventListener('keydown', function (e) { if (e.key === 'Escape') { q.value = ''; filter(); } });

  var toggle = document.getElementById('ds-theme');
  if (toggle) {
    try { if (localStorage.getItem('ds-theme') === 'dark') { root.dataset.theme = 'dark'; toggle.checked = true; } } catch (e) {}
    toggle.addEventListener('change', function () {
      root.dataset.theme = toggle.checked ? 'dark' : 'light';
      try { localStorage.setItem('ds-theme', toggle.checked ? 'dark' : 'light'); } catch (e) {}
    });
  }

  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (entry) {
      if (!entry.isIntersecting) { return; }
      subs.forEach(function (a) { a.removeAttribute('aria-current'); });
      var active = document.querySelector('.ds-subitem[href="#' + entry.target.id + '"]');
      if (active) { active.setAttribute('aria-current', 'page'); }
    });
  }, { rootMargin: '-10% 0px -80% 0px' });
  document.querySelectorAll('.ds-comp').forEach(function (s) { observer.observe(s); });
})();
