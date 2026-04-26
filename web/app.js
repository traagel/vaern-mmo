'use strict';

/* ─── Vaern Compendium ────────────────────────────────────────────────────
   Glossary-style data browser for src/generated/. Renders ten tabs and
   hash-routed detail pages. Visual language follows the warm-cold faction
   gradient established by the splash page (../index.html).
*/

const state = {
  data: null,
  currentTab: 'overview',
  filters: {
    spell:   { search: '', pillar: '', category: '', school: '', tier: '', morality: '', hasIcon: false },
    zone:    { search: '', faction: '', tier: '', biome: '' },
    dungeon: { search: '', band: '', size: '', kind: '' },
  },
};

const $  = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

// ─── helpers ────────────────────────────────────────────────────────────
function el(tag, attrs, ...kids) {
  const e = document.createElement(tag);
  if (attrs) {
    for (const [k, v] of Object.entries(attrs)) {
      if (v == null || v === false) continue;
      if (k === 'class') e.className = v;
      else if (k === 'html') e.innerHTML = v;
      else if (k === 'on') { for (const [ev, fn] of Object.entries(v)) e.addEventListener(ev, fn); }
      else e.setAttribute(k, v);
    }
  }
  for (const k of kids.flat()) {
    if (k == null || k === false) continue;
    e.appendChild(typeof k === 'string' ? document.createTextNode(k) : k);
  }
  return e;
}

function pretty(s) {
  if (s == null) return '';
  return String(s).replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function factionClass(f) {
  if (f === 'faction_a') return 'faction-a';
  if (f === 'faction_b') return 'faction-b';
  if (f === 'contested') return 'faction-contested';
  return '';
}
function factionLabel(f) {
  if (f === 'faction_a') return 'Concord';
  if (f === 'faction_b') return 'Rend';
  if (f === 'contested') return 'Contested';
  return f ? pretty(f) : '';
}

function chip(text, extra) { return el('span', { class: 'chip' + (extra ? ' ' + extra : '') }, text); }

function uniqueSorted(arr) { return Array.from(new Set(arr)).sort((a, b) => String(a).localeCompare(String(b))); }

function fillSelect(sel, values, labeler = (v) => pretty(v)) {
  const first = sel.firstElementChild;
  sel.innerHTML = '';
  if (first) sel.appendChild(first);
  for (const v of values) sel.appendChild(el('option', { value: v }, labeler(v)));
}

function clearChildren(node) { while (node.firstChild) node.removeChild(node.firstChild); }

function heroImage(paths) {
  // featured image + small clickable variant strip (if more than 1)
  if (!paths?.length) return null;
  const featured = el('img', { class: 'hero-img', src: ''+paths[0], alt: '' });
  const wrap = el('div', { class: 'hero-image-wrap' }, featured);
  if (paths.length > 1) {
    const strip = el('div', { class: 'hero-variants' });
    paths.forEach((p, i) => {
      const t = el('img', {
        class: 'hero-variant' + (i === 0 ? ' active' : ''),
        src: ''+p, alt: '',
      });
      t.addEventListener('click', () => {
        featured.src = ''+p;
        strip.querySelectorAll('.hero-variant').forEach((x) => x.classList.remove('active'));
        t.classList.add('active');
      });
      strip.appendChild(t);
    });
    wrap.appendChild(strip);
  }
  return wrap;
}

function hubThumb(paths, alt) {
  if (!paths?.length) return null;
  return el('div', { class: 'mc-thumb' },
    el('img', { src: ''+paths[0], alt: alt || '' }),
  );
}

function promptBlock(label, prompt, negative) {
  const onCopy = (text, btn) => {
    const reset = btn.textContent;
    navigator.clipboard.writeText(text).then(() => {
      btn.textContent = 'copied';
      setTimeout(() => { btn.textContent = reset; }, 1200);
    }).catch(() => { btn.textContent = 'copy failed'; });
  };
  const copyBtn = el('button', { class: 'prompt-copy', type: 'button' }, 'copy');
  copyBtn.addEventListener('click', (e) => { e.stopPropagation(); onCopy(prompt, copyBtn); });
  const header = el('div', { class: 'prompt-head' },
    el('span', { class: 'prompt-label' }, label),
    copyBtn,
  );
  const children = [header, el('div', { class: 'prompt-body' }, prompt)];
  if (negative) {
    const negCopy = el('button', { class: 'prompt-copy', type: 'button' }, 'copy');
    negCopy.addEventListener('click', (e) => { e.stopPropagation(); onCopy(negative, negCopy); });
    children.push(
      el('div', { class: 'prompt-head prompt-head-neg' },
        el('span', { class: 'prompt-label' }, 'negative'),
        negCopy,
      ),
      el('div', { class: 'prompt-body prompt-neg' }, negative),
    );
  }
  return el('div', { class: 'prompt-block' }, ...children);
}

// ─── boot ───────────────────────────────────────────────────────────────
async function load() {
  const r = await fetch('data.json');
  state.data = await r.json();
  renderStats();
  renderOverview();
  renderFactions();
  renderRaces();
  renderZones();
  renderBiomes();
  renderDungeons();
  renderInstitutions();
  renderClasses();
  renderSchools();
  populateSpellFilters();
  wireSpellFilters();
  renderSpells();
  wireTabs();
  wireOverlay();
  window.addEventListener('hashchange', route);
  route();
}

function renderStats() {
  const d = state.data;
  const c = d.counts || {};
  $('#stats').textContent =
    `${d.zones?.length ?? 0} zones · ${d.dungeons?.length ?? 0} dungeons · ${d.races?.length ?? 0} races · ` +
    `${c.spells_total ?? 0} spells`;
}

// ─── routing ────────────────────────────────────────────────────────────
const ENTITY_ROUTES = ['race', 'class', 'order', 'institution', 'zone', 'hub', 'landmark', 'biome', 'dungeon', 'faction', 'school'];
const TAB_ROUTES = ['overview', 'factions', 'races', 'zones', 'biomes', 'dungeons', 'institutions', 'classes', 'schools', 'spells'];

function route() {
  const hash = window.location.hash.replace(/^#/, '');
  const [kind, id] = hash.split('/');
  const pageEl = $('#page');

  if (ENTITY_ROUTES.includes(kind) && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    const renderer = ENTITY_PAGES[kind];
    if (renderer) renderer(id);
    window.scrollTo(0, 0);
    return;
  }
  document.body.classList.remove('page-mode');
  pageEl.classList.add('hidden');
  const wanted = TAB_ROUTES.includes(kind) ? kind : 'overview';
  activateTab(wanted);
}

function activateTab(name) {
  $$('nav#tabs button').forEach((x) => x.classList.toggle('active', x.dataset.tab === name));
  $$('main section.tab').forEach((t) => t.classList.toggle('active', t.id === `${name}-tab`));
  state.currentTab = name;
}

function wireTabs() {
  $$('nav#tabs button').forEach((b) => {
    b.addEventListener('click', () => { window.location.hash = b.dataset.tab; });
  });
}

function wireOverlay() {
  $('#detail-close').addEventListener('click', closeOverlay);
  $('#detail-overlay').addEventListener('click', (e) => { if (e.target.id === 'detail-overlay') closeOverlay(); });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeOverlay(); });
}
function openOverlay(node) {
  const body = $('#detail-body');
  clearChildren(body); body.appendChild(node);
  $('#detail-overlay').classList.remove('hidden');
}
function closeOverlay() { $('#detail-overlay').classList.add('hidden'); }

// ─── OVERVIEW ───────────────────────────────────────────────────────────
function renderOverview() {
  const d = state.data;
  const w = d.world || {};
  $('#overview-title').textContent = (w.setting_name || 'VAERN').toUpperCase();
  $('#overview-lede').textContent = w.design_reference
    ? w.design_reference
    : 'A hardcore co-op MMO. Two factions. One island.';

  const body = $('#overview-body');
  clearChildren(body);

  if (w.faction_split) {
    const fs = w.faction_split;
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Faction Split'),
      el('dl', null,
        ...Object.entries(fs).flatMap(([k, v]) => [
          el('dt', null, factionLabel(k)),
          el('dd', null, v),
        ]),
      ),
    ));
  }

  const facts = [];
  if (w.max_level) facts.push(['Max level', String(w.max_level)]);
  if (w.target_time_to_cap) {
    const t = w.target_time_to_cap;
    facts.push(['Target time to cap',
      `${t.played_hours_min ?? '?'}–${t.played_hours_max ?? '?'} h · ${t.days_active ?? ''}`]);
    if (t.basis) facts.push(['Basis', t.basis]);
  }
  if (facts.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'World'),
      el('dl', null, ...facts.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  }

  if (w.design_principles?.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Design Principles'),
      el('ul', null, ...w.design_principles.map((p) => el('li', null, p))),
    ));
  }

  if (d.continents?.length) {
    const cont = d.continents[0];
    body.appendChild(el('div', { class: 'page-section cold' },
      el('h3', null, `Continent — ${cont.name || pretty(cont.id)}`),
      el('dl', null,
        el('dt', null, 'Type'),    el('dd', null, cont.type || '—'),
        el('dt', null, 'Scale'),   el('dd', null, cont.scale || '—'),
        el('dt', null, 'Climate'), el('dd', null, cont.climate || '—'),
      ),
      cont.regions?.length ? el('div', { class: 'subgrid', style: 'margin-top:14px' },
        ...cont.regions.map((r) => el('div', { class: 'minicard ' + factionClass(r.faction) },
          el('div', { class: 'mc-tag' }, factionLabel(r.faction)),
          el('div', { class: 'mc-title' }, pretty(r.id)),
          el('div', { class: 'mc-meta' }, (r.biomes || []).map(pretty).join(' · ')),
        )),
      ) : null,
    ));
  }

  body.appendChild(el('div', { class: 'page-section' },
    el('h3', null, 'Browse'),
    el('div', { class: 'subgrid' }, ...[
      ['#factions',     'Factions',     `${d.factions?.length || 0} powers`],
      ['#races',        'Races',        `${d.races?.length || 0} peoples`],
      ['#zones',        'Zones',        `${d.zones?.length || 0} regions`],
      ['#biomes',       'Biomes',       `${d.biomes?.length || 0} biomes`],
      ['#dungeons',     'Dungeons',     `${d.dungeons?.length || 0} instances`],
      ['#institutions', 'Institutions', `${d.institutions?.length || 0} chartered`],
      ['#classes',      'Archetypes',   `${d.classes?.length || 0} core classes`],
      ['#schools',      'Schools',      `${d.schools?.length || 0} traditions`],
      ['#spells',       'Spellbook',    `${d.counts?.spells_total || 0} abilities`],
    ].map(([href, title, sub]) => el('a', { href, class: 'minicard', style: 'text-decoration:none' },
      el('div', { class: 'mc-tag' }, sub),
      el('div', { class: 'mc-title' }, title),
    ))),
  ));
}

// ─── FACTIONS ───────────────────────────────────────────────────────────
function renderFactions() {
  const grid = $('#faction-grid');
  clearChildren(grid);
  for (const f of state.data.factions || []) {
    const card = el('div', { class: 'card ' + factionClass(f.id) },
      f.has_emblem ? el('div', { class: 'emblem' }, el('img', { src: `emblems/${f.id}.png`, alt: '' })) : null,
      el('div', { class: 'card-title' }, factionLabel(f.id)),
      el('div', { class: 'card-subtitle' }, pretty(f.morality_alignment || '')),
      el('div', { class: 'card-desc' }, f.visual?.pitch || ''),
      el('div', { class: 'card-meta' },
        chip(`${(f.allowed_school_moralities || []).length} moralities`),
        chip(`${flattenSchools(f.allowed_schools).length} schools`),
      ),
    );
    card.addEventListener('click', () => { window.location.hash = `faction/${f.id}`; });
    grid.appendChild(card);
  }
}

function flattenSchools(map) {
  if (!map) return [];
  return Object.values(map).flat();
}

function renderFactionPage(id) {
  const f = (state.data.factions || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!f) { body.appendChild(el('div', null, 'Faction not found.')); return; }
  $('#page-back').href = '#factions';
  const cls = factionClass(f.id);
  body.appendChild(el('div', { class: 'page-hero' },
    f.has_emblem ? el('div', { class: 'portrait-slot', style: 'aspect-ratio:1/1; width:200px' },
      el('img', { src: `emblems/${f.id}.png`, alt: '' })) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, factionLabel(f.id)),
      el('div', { class: 'page-subtitle' }, pretty(f.morality_alignment || '')),
      el('p', { class: 'page-lede' }, f.visual?.pitch || ''),
    ),
  ));

  if (f.visual) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Visual Identity'),
      el('dl', null,
        ...Object.entries(f.visual)
          .filter(([k]) => k !== 'pitch' && k !== 'negative_tags')
          .flatMap(([k, v]) => [el('dt', null, pretty(k)), el('dd', null, String(v))]),
      ),
    ));
  }

  if (f.allowed_schools) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Allowed Schools'),
      el('dl', null,
        ...Object.entries(f.allowed_schools).flatMap(([pillar, schools]) => [
          el('dt', null, pretty(pillar)),
          el('dd', null, schools.map(pretty).join(' · ')),
        ]),
      ),
    ));
  }
  if (f.forbidden_schools && Object.keys(f.forbidden_schools).length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Forbidden Schools'),
      el('dl', null,
        ...Object.entries(f.forbidden_schools).flatMap(([pillar, schools]) => [
          el('dt', null, pretty(pillar)),
          el('dd', { class: 'morality-evil' }, schools.map(pretty).join(' · ')),
        ]),
      ),
    ));
  }
}

// ─── RACES ──────────────────────────────────────────────────────────────
function renderRaces() {
  const grid = $('#race-grid');
  grid.className = 'vstack';
  clearChildren(grid);
  for (const r of state.data.races || []) {
    const portraitSrc = pickRacePortrait(r);
    const stats = [
      pretty(r.creature_type || 'humanoid'),
      pretty(r.size_class || 'medium'),
      r.archetype ? pretty(r.archetype) : null,
    ].filter(Boolean).join(' · ');
    const row = el('div', { class: 'vrow race-row ' + factionClass(r.faction) },
      el('div', { class: 'vrow-thumb' },
        portraitSrc ? el('img', { src: portraitSrc, alt: '' }) : 'no portrait'),
      el('div', { class: 'vrow-text' },
        el('div', { class: 'vrow-tag' }, factionLabel(r.faction)),
        el('div', { class: 'vrow-title' }, r.id ? pretty(r.id) : '?'),
        el('div', { class: 'vrow-desc' }, r.lore_hook || r.cultural_traits || ''),
        stats ? el('div', { class: 'vrow-stats' }, stats) : null,
      ),
    );
    row.addEventListener('click', () => { window.location.hash = `race/${r.id}`; });
    grid.appendChild(row);
  }
}

function pickRacePortrait(r) {
  const p = r.portraits || {};
  // portraits values are now repo-relative paths (or null) — preferred
  // order: male, female, neutral
  return p.male || p.female || p.neutral
    ? ''+(p.male || p.female || p.neutral)
    : null;
}

function renderRacePage(id) {
  const r = (state.data.races || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!r) { body.appendChild(el('div', null, 'Race not found.')); return; }
  $('#page-back').href = '#races';
  const cls = factionClass(r.faction);

  const portraits = [];
  if (r.portraits?.male)    portraits.push(['Male',    ''+r.portraits.male]);
  if (r.portraits?.female)  portraits.push(['Female',  ''+r.portraits.female]);
  if (r.portraits?.neutral) portraits.push(['',        ''+r.portraits.neutral]);

  const heroPortrait = portraits.length
    ? el('div', { class: 'portrait-slot' },
        el('img', { src: portraits[0][1], alt: '' }),
        portraits[0][0] ? el('div', { class: 'label' }, portraits[0][0]) : null)
    : el('div', { class: 'portrait-slot' }, 'no portrait');

  body.appendChild(el('div', { class: 'page-hero' },
    heroPortrait,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, pretty(r.id)),
      el('div', { class: 'page-subtitle' },
        `${factionLabel(r.faction)} · ${pretty(r.archetype || 'human')} · ${pretty(r.size_class || '')}`),
      r.lore_hook ? el('p', { class: 'page-lede' }, r.lore_hook) : null,
      r.cultural_traits ? el('p', { class: 'page-lede' }, r.cultural_traits) : null,
    ),
  ));

  if (portraits.length > 1) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Variants'),
      el('div', { class: 'subgrid' },
        ...portraits.map(([lbl, src]) => el('div', { class: 'minicard' },
          el('div', { class: 'portrait' }, el('img', { src, alt: '' })),
          lbl ? el('div', { class: 'mc-tag', style: 'margin-top:6px' }, lbl) : null,
        )),
      ),
    ));
  }

  if (r.affinity) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Pillar Affinity'),
      el('dl', null,
        el('dt', null, 'Might'),   el('dd', null, String(r.affinity.might   ?? '—')),
        el('dt', null, 'Arcana'),  el('dd', null, String(r.affinity.arcana  ?? '—')),
        el('dt', null, 'Finesse'), el('dd', null, String(r.affinity.finesse ?? '—')),
        r.affinity.notes ? el('dt', null, 'Notes') : null,
        r.affinity.notes ? el('dd', null, r.affinity.notes) : null,
      ),
    ));
  }

  const mech = [];
  if (r.creature_type) mech.push(['Creature type', pretty(r.creature_type)]);
  if (r.size_class)    mech.push(['Size class',    pretty(r.size_class)]);
  if (r.hp_modifier != null) mech.push(['HP modifier', `×${r.hp_modifier}`]);
  if (r.favored_class) mech.push(['Favored class',  pretty(r.favored_class)]);
  if (mech.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Mechanics'),
      el('dl', null, ...mech.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  }

  if (r.visual) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Appearance'),
      el('dl', null,
        ...Object.entries(r.visual)
          .filter(([k]) => k !== 'tags' && k !== 'negative_tags')
          .flatMap(([k, v]) => [el('dt', null, pretty(k)), el('dd', null, String(v))]),
      ),
    ));
  }
}

// ─── ZONES ──────────────────────────────────────────────────────────────
function renderZones() {
  const z = state.data.zones || [];
  fillSelect($('#zone-faction'), uniqueSorted(z.map((x) => x.faction_control).filter(Boolean)), factionLabel);
  fillSelect($('#zone-tier'),    uniqueSorted(z.map((x) => x.tier).filter(Boolean)));
  fillSelect($('#zone-biome'),   uniqueSorted(z.map((x) => x.biome).filter(Boolean)));

  const f = state.filters.zone;
  $('#zone-search').addEventListener('input',  (e) => { f.search  = e.target.value.toLowerCase(); paintZones(); });
  $('#zone-faction').addEventListener('change', (e) => { f.faction = e.target.value; paintZones(); });
  $('#zone-tier').addEventListener('change',    (e) => { f.tier    = e.target.value; paintZones(); });
  $('#zone-biome').addEventListener('change',   (e) => { f.biome   = e.target.value; paintZones(); });
  paintZones();
}

function paintZones() {
  const grid = $('#zone-grid');
  grid.className = 'vstack';
  clearChildren(grid);
  const f = state.filters.zone;
  let count = 0;
  for (const z of state.data.zones || []) {
    if (f.faction && z.faction_control !== f.faction) continue;
    if (f.tier && z.tier !== f.tier) continue;
    if (f.biome && z.biome !== f.biome) continue;
    if (f.search) {
      const s = ((z.name || '') + ' ' + (z.notes || '') + ' ' + (z.id || '')).toLowerCase();
      if (!s.includes(f.search)) continue;
    }
    count++;
    const stats = [
      z.tier ? pretty(z.tier) : null,
      z.level_range ? `Lv ${z.level_range.min}-${z.level_range.max}` : null,
      z.hub_count ? `${z.hub_count} hubs` : null,
    ].filter(Boolean).join(' · ');
    const row = el('div', { class: 'vrow ' + factionClass(z.faction_control) },
      el('div', { class: 'vrow-thumb' },
        z.images?.length ? el('img', { src: ''+z.images[0], alt: '' }) : 'no image'),
      el('div', { class: 'vrow-text' },
        el('div', { class: 'vrow-tag' },
          `${factionLabel(z.faction_control)} · ${pretty(z.biome || '—')}`),
        el('div', { class: 'vrow-title' }, z.name || pretty(z.id)),
        el('div', { class: 'vrow-desc' }, z.description || z.notes || ''),
        stats ? el('div', { class: 'vrow-stats' }, stats) : null,
      ),
    );
    row.addEventListener('click', () => { window.location.hash = `zone/${z.id}`; });
    grid.appendChild(row);
  }
  $('#zone-count').textContent = `${count} of ${(state.data.zones || []).length}`;
}

function renderZonePage(id) {
  const z = (state.data.zones || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!z) { body.appendChild(el('div', null, 'Zone not found.')); return; }
  $('#page-back').href = '#zones';
  const cls = factionClass(z.faction_control);

  body.appendChild(el('div', { class: 'page-hero' },
    z.images?.length ? heroImage(z.images) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, z.name || pretty(z.id)),
      el('div', { class: 'page-subtitle' },
        `${factionLabel(z.faction_control)} · ${pretty(z.biome || '')} · ${pretty(z.region || '')}`),
      z.vibe ? el('div', { class: 'page-vibe' }, z.vibe) : null,
      z.description ? el('p', { class: 'page-lede' }, z.description) : null,
      z.notes && z.notes !== z.description ? el('p', { class: 'page-notes' }, z.notes) : null,
      z.prompt ? promptBlock('SDXL prompt', z.prompt, z.negative_prompt) : null,
    ),
  ));

  const facts = [];
  if (z.tier) facts.push(['Tier', pretty(z.tier)]);
  if (z.level_range) facts.push(['Level range', `${z.level_range.min} – ${z.level_range.max}`]);
  if (z.starter_race) facts.push(['Starter race', pretty(z.starter_race)]);
  if (z.hub_count) facts.push(['Hubs', String(z.hub_count)]);
  if (z.budget) {
    if (z.budget.quest_count_target)     facts.push(['Quests (target)', String(z.budget.quest_count_target)]);
    if (z.budget.unique_mob_types)       facts.push(['Unique mobs',     String(z.budget.unique_mob_types)]);
    if (z.budget.mob_kills_to_complete)  facts.push(['Kills to complete', String(z.budget.mob_kills_to_complete)]);
    if (z.budget.estimated_hours_to_complete) {
      const h = z.budget.estimated_hours_to_complete;
      facts.push(['Estimated hours', `solo ${h.solo} · duo ${h.duo}`]);
    }
  }
  if (facts.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Overview'),
      el('dl', null, ...facts.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  }

  if (z.hubs?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Hubs (${z.hubs.length})`),
      el('div', { class: 'vstack' },
        ...z.hubs.map((h) => {
          const stats = (h.amenities || []).map(pretty).join(' · ')
            + (h.quest_givers ? ` · ${h.quest_givers} qg` : '')
            + (h.prop_count ? ` · ${h.prop_count} props` : '');
          const row = el('a', { class: 'vrow', href: `#hub/${h.id}` },
            el('div', { class: 'vrow-thumb' },
              h.images?.length ? el('img', { src: ''+h.images[0], alt: '' }) : 'no image',
            ),
            el('div', { class: 'vrow-text' },
              el('div', { class: 'vrow-tag' }, pretty(h.role || h.biome || 'hub')),
              el('div', { class: 'vrow-title' }, h.name || pretty(h.id)),
              h.description ? el('div', { class: 'vrow-desc' }, h.description) : null,
              stats ? el('div', { class: 'vrow-stats' }, stats) : null,
            ),
          );
          return row;
        }),
      ),
    ));
  }

  if (z.landmarks?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Landmarks (${z.landmarks.length})`),
      el('div', { class: 'vstack' },
        ...z.landmarks.map((lm) => {
          const off = lm.offset_from_zone_origin;
          const coord = off ? `(${off.x ?? 0}, ${off.z ?? 0})` : '';
          return el('a', { class: 'vrow', href: `#landmark/${lm.id}` },
            el('div', { class: 'vrow-thumb' },
              lm.images?.length ? el('img', { src: ''+lm.images[0], alt: '' }) : 'no image',
            ),
            el('div', { class: 'vrow-text' },
              el('div', { class: 'vrow-tag' }, 'landmark'),
              el('div', { class: 'vrow-title' }, lm.name || pretty(lm.id)),
              lm.description ? el('div', { class: 'vrow-desc' }, lm.description) : null,
              coord ? el('div', { class: 'vrow-stats' }, coord) : null,
            ),
          );
        }),
      ),
    ));
  }

  if (z.scatter_categories?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'World-dressing'),
      el('dl', null,
        el('dt', null, 'Scatter categories'),
        el('dd', null, z.scatter_categories.map(pretty).join(' · ')),
      ),
    ));
  }

  const dungs = (state.data.dungeons || []).filter((d) => d.zone === z.id);
  if (dungs.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Dungeons (${dungs.length})`),
      el('div', { class: 'vstack' },
        ...dungs.map((d) => el('a', { class: 'vrow', href: `#dungeon/${d.id}` },
          el('div', { class: 'vrow-thumb' },
            d.images?.length ? el('img', { src: ''+d.images[0], alt: '' }) : 'no image',
          ),
          el('div', { class: 'vrow-text' },
            el('div', { class: 'vrow-tag' }, `${pretty(d.kind || 'dungeon')} · ${d.group_size || '?'}p · Lv ${d.level_range?.min}-${d.level_range?.max}`),
            el('div', { class: 'vrow-title' }, d.name || pretty(d.id)),
            d.theme ? el('div', { class: 'vrow-desc' }, d.theme) : null,
            el('div', { class: 'vrow-stats' },
              `${d.boss_count || 0} bosses · ~${d.estimated_clear_minutes || '?'} min`
              + (d.loot_tier ? ` · loot: ${d.loot_tier}` : '')
              + (d.tier ? ` · ${pretty(d.tier)}` : '')
            ),
          ),
        )),
      ),
    ));
  }
}

// ─── HUB DETAIL ─────────────────────────────────────────────────────────
function findHubById(id) {
  for (const z of state.data.zones || []) {
    const h = (z.hubs || []).find((x) => x.id === id);
    if (h) return { hub: h, zone: z };
  }
  return null;
}

function renderHubPage(id) {
  const hit = findHubById(id);
  const body = $('#page-body'); clearChildren(body);
  if (!hit) { body.appendChild(el('div', null, 'Hub not found.')); return; }
  const { hub: h, zone: z } = hit;
  $('#page-back').href = `#zone/${z.id}`;
  const cls = factionClass(z.faction_control);

  body.appendChild(el('div', { class: 'page-hero' },
    h.images?.length ? heroImage(h.images) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, h.name || pretty(h.id)),
      el('div', { class: 'page-subtitle' },
        `${pretty(h.role || 'hub')} · ${z.name || pretty(z.id)} · ${factionLabel(z.faction_control)}`),
      h.description ? el('p', { class: 'page-lede' }, h.description) : null,
      h.prompt ? promptBlock('SDXL prompt', h.prompt, h.negative_prompt) : null,
    ),
  ));

  const facts = [];
  if (h.amenities?.length) facts.push(['Amenities', h.amenities.map(pretty).join(' · ')]);
  if (h.quest_givers != null) facts.push(['Quest givers', String(h.quest_givers)]);
  if (h.prop_count != null) facts.push(['Authored props', String(h.prop_count)]);
  if (h.biome) facts.push(['Local biome', pretty(h.biome)]);
  if (h.offset_from_zone_origin) {
    const o = h.offset_from_zone_origin;
    facts.push(['Offset from zone origin', `(${o.x ?? 0}, ${o.z ?? 0})`]);
  }
  if (facts.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Overview'),
      el('dl', null, ...facts.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  }

  // Sibling hubs in the same zone
  const siblings = (z.hubs || []).filter((x) => x.id !== h.id);
  if (siblings.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Other hubs in ${z.name || pretty(z.id)}`),
      el('div', { class: 'subgrid' },
        ...siblings.map((s) => el('a', { class: 'minicard', href: `#hub/${s.id}`, style: 'text-decoration:none' },
          s.images?.length ? hubThumb(s.images, s.name) : null,
          el('div', { class: 'mc-tag' }, pretty(s.role || 'hub')),
          el('div', { class: 'mc-title' }, s.name || pretty(s.id)),
        )),
      ),
    ));
  }
}

// ─── LANDMARK DETAIL ────────────────────────────────────────────────────
function findLandmarkById(id) {
  for (const z of state.data.zones || []) {
    const lm = (z.landmarks || []).find((x) => x.id === id);
    if (lm) return { landmark: lm, zone: z };
  }
  return null;
}

function renderLandmarkPage(id) {
  const hit = findLandmarkById(id);
  const body = $('#page-body'); clearChildren(body);
  if (!hit) { body.appendChild(el('div', null, 'Landmark not found.')); return; }
  const { landmark: lm, zone: z } = hit;
  $('#page-back').href = `#zone/${z.id}`;
  const cls = factionClass(z.faction_control);

  body.appendChild(el('div', { class: 'page-hero' },
    lm.images?.length ? heroImage(lm.images) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, lm.name || pretty(lm.id)),
      el('div', { class: 'page-subtitle' },
        `Landmark · ${z.name || pretty(z.id)} · ${factionLabel(z.faction_control)}`),
      lm.description ? el('p', { class: 'page-lede' }, lm.description) : null,
      lm.prompt ? promptBlock('SDXL prompt', lm.prompt, lm.negative_prompt) : null,
    ),
  ));

  const facts = [];
  if (lm.offset_from_zone_origin) {
    const o = lm.offset_from_zone_origin;
    facts.push(['Offset from zone origin', `(${o.x ?? 0}, ${o.z ?? 0})`]);
  }
  if (lm.biome) facts.push(['Local biome', pretty(lm.biome)]);
  if (lm.level_range) facts.push(['Level range', `${lm.level_range.min} – ${lm.level_range.max}`]);
  if (facts.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Overview'),
      el('dl', null, ...facts.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  } else {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Notes'),
      el('p', { style: 'color:var(--parchment-dim); font-style:italic' },
        'No description or prompt authored yet — landmarks accept the same '
        + '`description:` and `prompt:` fields as zones and hubs.'),
    ));
  }
}

// ─── BIOMES ─────────────────────────────────────────────────────────────
function renderBiomes() {
  const grid = $('#biome-grid');
  clearChildren(grid);
  // switch this tab from a grid to a vertical stack
  grid.className = 'vstack';
  const zones = state.data.zones || [];
  for (const b of state.data.biomes || []) {
    const inBiome = zones.filter((z) => z.biome === b.id);
    const factionTag = (factionLabel(b.faction_affinity) || 'neutral')
      + (b.climate ? ` · ${b.climate}` : '');
    const stats = (b.hazards || []).map(pretty).join(' · ');

    let zonesBlock = null;
    if (inBiome.length) {
      const tiles = inBiome.map((z) => {
        const tile = el('a', {
          class: 'vrow-zone-tile ' + factionClass(z.faction_control),
          href: `#zone/${z.id}`,
        },
          el('div', { class: 'vrow-zone-tile-thumb' },
            z.images?.length
              ? el('img', { src: ''+z.images[0], alt: '' })
              : null,
          ),
          el('div', { class: 'vrow-zone-tile-name' }, z.name || pretty(z.id)),
        );
        tile.addEventListener('click', (e) => e.stopPropagation());
        return tile;
      });
      zonesBlock = el('div', { class: 'vrow-zones' },
        el('div', { class: 'vrow-zones-label' },
          `Includes ${inBiome.length} zone${inBiome.length === 1 ? '' : 's'}`),
        el('div', { class: 'vrow-zone-tiles' }, ...tiles),
      );
    } else {
      zonesBlock = el('div', { class: 'vrow-zones vrow-zones-empty' }, 'Not used by any zone yet');
    }

    const row = el('div', { class: 'vrow ' + factionClass(b.faction_affinity) },
      el('div', { class: 'vrow-thumb' },
        b.images?.length ? el('img', { src: ''+b.images[0], alt: '' }) : 'no image',
      ),
      el('div', { class: 'vrow-text' },
        el('div', { class: 'vrow-tag' }, factionTag),
        el('div', { class: 'vrow-title' }, b.name || pretty(b.id)),
        b.description ? el('div', { class: 'vrow-desc' }, b.description) : null,
        zonesBlock,
        stats ? el('div', { class: 'vrow-stats' }, stats) : null,
      ),
    );
    row.addEventListener('click', () => { window.location.hash = `biome/${b.id}`; });
    grid.appendChild(row);
  }
}

function renderBiomePage(id) {
  const b = (state.data.biomes || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!b) { body.appendChild(el('div', null, 'Biome not found.')); return; }
  $('#page-back').href = '#biomes';
  const cls = factionClass(b.faction_affinity);

  body.appendChild(el('div', { class: 'page-hero' },
    b.images?.length ? heroImage(b.images) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, b.name || pretty(b.id)),
      el('div', { class: 'page-subtitle' },
        `${factionLabel(b.faction_affinity) || 'neutral'} · ${b.climate || ''}`),
      b.description ? el('p', { class: 'page-lede' }, b.description) : null,
      b.prompt ? promptBlock('SDXL prompt', b.prompt, b.negative_prompt) : null,
    ),
  ));

  if (b.hazards?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Hazards'),
      el('ul', null, ...b.hazards.map((h) => el('li', null, h))),
    ));
  }
  if (b.typical_flora?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Typical Flora'),
      el('div', null, b.typical_flora.map(pretty).join(' · ')),
    ));
  }
  if (b.typical_fauna?.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Typical Fauna'),
      el('div', null, b.typical_fauna.map(pretty).join(' · ')),
    ));
  }

  const zones = (state.data.zones || []).filter((z) => z.biome === b.id);
  if (zones.length) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Zones (${zones.length})`),
      el('div', { class: 'subgrid' },
        ...zones.map((z) => el('a', {
          class: 'minicard ' + factionClass(z.faction_control),
          href: `#zone/${z.id}`, style: 'text-decoration:none',
        },
          el('div', { class: 'mc-tag' }, factionLabel(z.faction_control)),
          el('div', { class: 'mc-title' }, z.name || pretty(z.id)),
          el('div', { class: 'mc-meta' }, z.level_range ? `Lv ${z.level_range.min}-${z.level_range.max}` : ''),
        )),
      ),
    ));
  }
}

// ─── DUNGEONS ───────────────────────────────────────────────────────────
function renderDungeons() {
  const d = state.data.dungeons || [];
  fillSelect($('#dungeon-band'), uniqueSorted(d.map((x) => x.level_band).filter(Boolean)));
  fillSelect($('#dungeon-size'), uniqueSorted(d.map((x) => x.group_size).filter((v) => v != null)),
    (v) => `${v}-player`);
  fillSelect($('#dungeon-kind'), uniqueSorted(d.map((x) => x.kind).filter(Boolean)));

  const f = state.filters.dungeon;
  $('#dungeon-search').addEventListener('input',  (e) => { f.search = e.target.value.toLowerCase(); paintDungeons(); });
  $('#dungeon-band').addEventListener('change',   (e) => { f.band   = e.target.value; paintDungeons(); });
  $('#dungeon-size').addEventListener('change',   (e) => { f.size   = e.target.value; paintDungeons(); });
  $('#dungeon-kind').addEventListener('change',   (e) => { f.kind   = e.target.value; paintDungeons(); });
  paintDungeons();
}

function paintDungeons() {
  const grid = $('#dungeon-grid');
  grid.className = 'vstack';
  clearChildren(grid);
  const f = state.filters.dungeon;
  let count = 0;
  for (const d of state.data.dungeons || []) {
    if (f.band && d.level_band !== f.band) continue;
    if (f.size && String(d.group_size) !== f.size) continue;
    if (f.kind && d.kind !== f.kind) continue;
    if (f.search) {
      const s = ((d.name || '') + ' ' + (d.theme || '') + ' ' + (d.id || '')).toLowerCase();
      if (!s.includes(f.search)) continue;
    }
    count++;
    const tier = d.tier === 'endgame' ? 'pillar-arcana' :
                 d.tier === 'midgame' ? 'pillar-finesse' :
                 d.tier === 'leveling' ? 'pillar-might' : '';
    const stats = [
      d.boss_count ? `${d.boss_count} bosses` : null,
      d.estimated_clear_minutes ? `~${d.estimated_clear_minutes} min` : null,
      d.loot_tier ? `loot: ${d.loot_tier}` : null,
    ].filter(Boolean).join(' · ');
    const row = el('div', { class: 'vrow ' + tier },
      el('div', { class: 'vrow-thumb' },
        d.images?.length ? el('img', { src: ''+d.images[0], alt: '' }) : 'no image'),
      el('div', { class: 'vrow-text' },
        el('div', { class: 'vrow-tag' },
          `${pretty(d.kind || 'dungeon')} · ${d.group_size || '?'}-player · Lv ${d.level_range?.min}-${d.level_range?.max}`
          + (d.tier ? ` · ${pretty(d.tier)}` : '')),
        el('div', { class: 'vrow-title' }, d.name || pretty(d.id)),
        el('div', { class: 'vrow-desc' }, d.description || d.theme || ''),
        stats ? el('div', { class: 'vrow-stats' }, stats) : null,
      ),
    );
    row.addEventListener('click', () => { window.location.hash = `dungeon/${d.id}`; });
    grid.appendChild(row);
  }
  $('#dungeon-count').textContent = `${count} of ${(state.data.dungeons || []).length}`;
}

function renderDungeonPage(id) {
  const d = (state.data.dungeons || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!d) { body.appendChild(el('div', null, 'Dungeon not found.')); return; }
  $('#page-back').href = '#dungeons';

  body.appendChild(el('div', { class: 'page-hero' },
    d.images?.length ? heroImage(d.images) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title' }, d.name || pretty(d.id)),
      el('div', { class: 'page-subtitle' },
        `${pretty(d.kind || 'dungeon')} · ${d.group_size || '?'}-player · Lv ${d.level_range?.min}-${d.level_range?.max} · ${pretty(d.tier || '')}`),
      d.description ? el('p', { class: 'page-lede' }, d.description) : null,
      d.theme && d.theme !== d.description ? el('p', { class: 'page-notes' }, d.theme) : null,
      d.prompt ? promptBlock('SDXL prompt', d.prompt, d.negative_prompt) : null,
    ),
  ));

  const facts = [];
  const zoneEntry = (state.data.zones || []).find((z) => z.id === d.zone);
  if (d.zone) facts.push(['Zone', zoneEntry?.name || pretty(d.zone)]);
  if (d.entrance_hub) facts.push(['Entrance hub', pretty(d.entrance_hub)]);
  if (d.level_band) facts.push(['Level band', d.level_band]);
  if (d.boss_count != null) facts.push(['Boss count', String(d.boss_count)]);
  if (d.estimated_clear_minutes != null) facts.push(['Estimated clear', `${d.estimated_clear_minutes} min`]);
  if (d.loot_tier) facts.push(['Loot tier', pretty(d.loot_tier)]);
  if (d.lockout) facts.push(['Lockout', d.lockout]);
  if (d.coop_notes) facts.push(['Co-op notes', d.coop_notes]);
  if (facts.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Overview'),
      el('dl', null, ...facts.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
    ));
  }

  if (d.bosses?.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, `Bosses (${d.bosses.length})`),
      el('div', { class: 'vstack' },
        ...d.bosses.map((b) => el('div', { class: 'vrow' },
          el('div', { class: 'vrow-thumb' },
            b.images?.length ? el('img', { src: ''+b.images[0], alt: '' }) : 'no portrait',
          ),
          el('div', { class: 'vrow-text' },
            el('div', { class: 'vrow-tag' },
              `${pretty(b.role_tag || 'boss')} · Lv ${b.level || '?'} · ${pretty(b.hp_tier || '')}`),
            el('div', { class: 'vrow-title' }, b.name || pretty(b.id)),
            b.description ? el('div', { class: 'vrow-desc' }, b.description) : null,
            b.mechanic ? el('div', { class: 'vrow-stats' }, `Mechanic: ${b.mechanic}`) : null,
          ),
        )),
      ),
    ));
  }

  if (d.loot && Object.keys(d.loot).length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Loot'),
      el('pre', {
        style: 'background:rgba(20,20,26,.6); border:1px solid var(--line); padding:12px; border-radius:2px; font-size:12px; line-height:1.5; color:var(--parchment-dim); overflow:auto;',
      }, JSON.stringify(d.loot, null, 2)),
    ));
  }
}

// ─── INSTITUTIONS ───────────────────────────────────────────────────────
function renderInstitutions() {
  const grid = $('#institution-grid');
  clearChildren(grid);
  for (const i of state.data.institutions || []) {
    const card = el('div', { class: 'card ' + factionClass(i.faction) },
      i.has_emblem ? el('div', { class: 'emblem' },
        el('img', { src: `emblems/institution_${i.id}.png`, alt: '' })) : null,
      el('div', { class: 'card-title' }, i.name || pretty(i.id)),
      el('div', { class: 'card-subtitle' },
        `${factionLabel(i.faction)} · ${(i.pillars || []).map(pretty).join(' / ') || ''}`),
      el('div', { class: 'card-desc' }, i.tradition || i.lore?.description || ''),
      el('div', { class: 'card-meta' },
        i.chapters?.length ? chip(`${i.chapters.length} chapters`) : null,
        i.orders_produced?.length ? chip(`${i.orders_produced.length} orders`) : null,
      ),
    );
    card.addEventListener('click', () => { window.location.hash = `institution/${i.id}`; });
    grid.appendChild(card);
  }
}

function renderInstitutionPage(id) {
  const i = (state.data.institutions || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!i) { body.appendChild(el('div', null, 'Institution not found.')); return; }
  $('#page-back').href = '#institutions';
  const cls = factionClass(i.faction);

  body.appendChild(el('div', { class: 'page-hero' },
    i.has_emblem ? el('div', { class: 'portrait-slot', style: 'aspect-ratio:1/1; width:200px' },
      el('img', { src: `emblems/institution_${i.id}.png`, alt: '' })) : null,
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title ' + cls }, i.name || pretty(i.id)),
      el('div', { class: 'page-subtitle' },
        `${factionLabel(i.faction)} · ${(i.pillars || []).map(pretty).join(' / ')}`),
      i.lore?.description ? el('p', { class: 'page-lede' }, i.lore.description) : null,
    ),
  ));

  if (i.tradition || i.lore?.doctrine || i.lore?.home || i.lore?.patron) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Doctrine'),
      el('dl', null,
        i.tradition ? el('dt', null, 'Tradition') : null,
        i.tradition ? el('dd', null, i.tradition) : null,
        i.lore?.doctrine ? el('dt', null, 'Doctrine') : null,
        i.lore?.doctrine ? el('dd', null, i.lore.doctrine) : null,
        i.lore?.home ? el('dt', null, 'Home') : null,
        i.lore?.home ? el('dd', null, i.lore.home) : null,
        i.lore?.patron ? el('dt', null, 'Patron') : null,
        i.lore?.patron ? el('dd', null, i.lore.patron) : null,
        i.lore?.founded ? el('dt', null, 'Founded') : null,
        i.lore?.founded ? el('dd', null, i.lore.founded) : null,
        i.lore?.recruitment ? el('dt', null, 'Recruitment') : null,
        i.lore?.recruitment ? el('dd', null, i.lore.recruitment) : null,
      ),
    ));
  }

  if (i.curriculum) {
    const rows = [];
    for (const [tier, byPillar] of Object.entries(i.curriculum)) {
      for (const [pillar, schools] of Object.entries(byPillar || {})) {
        rows.push([`${pretty(tier)} · ${pretty(pillar)}`, (schools || []).map(pretty).join(' · ')]);
      }
    }
    if (rows.length) {
      body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
        el('h3', null, 'Curriculum'),
        el('dl', null, ...rows.flatMap(([k, v]) => [el('dt', null, k), el('dd', null, v)])),
      ));
    }
  }

  if (i.aesthetic) {
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, 'Aesthetic'),
      el('dl', null,
        ...Object.entries(i.aesthetic).flatMap(([k, v]) => [el('dt', null, pretty(k)), el('dd', null, String(v))]),
      ),
    ));
  }

  if (i.orders_produced?.length) {
    const ordersById = new Map((state.data.orders || []).map((o) => [o.id, o]));
    body.appendChild(el('div', { class: 'page-section' + (cls === 'faction-b' ? ' cold' : '') },
      el('h3', null, `Orders Produced (${i.orders_produced.length})`),
      el('div', { class: 'subgrid' },
        ...i.orders_produced.map((oid) => {
          const o = ordersById.get(oid);
          return el('a', {
            class: 'minicard', href: `#order/${oid}`, style: 'text-decoration:none',
          },
            el('div', { class: 'mc-tag' }, o ? `class ${o.class_id ?? '?'}` : 'order'),
            el('div', { class: 'mc-title' }, o?.name || pretty(oid)),
            o?.flavor ? el('div', { class: 'mc-meta' }, o.flavor) : null,
          );
        }),
      ),
    ));
  }
}

// ─── CLASSES (archetypes) ───────────────────────────────────────────────
function renderClasses() {
  const grid = $('#class-grid');
  clearChildren(grid);
  for (const c of state.data.classes || []) {
    const dom = (c.dominant_pillar || [])[0] || '';
    const card = el('div', { class: 'card class-card pillar-' + (dom || 'arcana') },
      el('div', { class: 'card-title' }, c.abstract_name || pretty(c.internal_label || '')),
      el('div', { class: 'card-subtitle' },
        `${pretty(c.pillar_classification || '')} · ${(c.dominant_pillar || []).map(pretty).join(' / ')}`),
      el('div', { class: 'card-desc' }, c.note || c.edge || ''),
      el('div', { class: 'card-meta' },
        ...(c.primary_roles || []).map((r) => chip(pretty(r))),
      ),
    );
    card.addEventListener('click', () => { window.location.hash = `class/${c.class_id}`; });
    grid.appendChild(card);
  }
}

function renderClassPage(id) {
  const c = (state.data.classes || []).find((x) => x.class_id === Number(id) || String(x.class_id) === id);
  const body = $('#page-body'); clearChildren(body);
  if (!c) { body.appendChild(el('div', null, 'Class not found.')); return; }
  $('#page-back').href = '#classes';

  body.appendChild(el('div', { class: 'page-hero' },
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title' }, c.abstract_name || pretty(c.internal_label || '')),
      el('div', { class: 'page-subtitle' },
        `Class ${c.class_id} · ${pretty(c.pillar_classification || '')} · ${(c.dominant_pillar || []).map(pretty).join(' / ')}`),
      c.note ? el('p', { class: 'page-lede' }, c.note) : null,
    ),
  ));

  if (c.position) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Pillar Position'),
      el('dl', null,
        el('dt', null, 'Might'),   el('dd', null, String(c.position.might   ?? 0)),
        el('dt', null, 'Arcana'),  el('dd', null, String(c.position.arcana  ?? 0)),
        el('dt', null, 'Finesse'), el('dd', null, String(c.position.finesse ?? 0)),
        c.edge ? el('dt', null, 'Edge') : null,
        c.edge ? el('dd', null, c.edge) : null,
      ),
    ));
  }

  if (c.capabilities) {
    const rows = [];
    for (const [pillar, caps] of Object.entries(c.capabilities)) {
      if (!caps?.length) continue;
      rows.push([pretty(pillar), caps]);
    }
    if (rows.length) {
      body.appendChild(el('div', { class: 'page-section' },
        el('h3', null, 'Capabilities'),
        ...rows.map(([k, list]) => el('div', null,
          el('div', { style: 'font-family:Cinzel,serif; letter-spacing:.2em; text-transform:uppercase; color:var(--parchment-faint); font-size:.7rem; margin-top:6px' }, k),
          el('ul', null, ...list.map((x) => el('li', null, x))),
        )),
      ));
    }
  }

  if (c.references) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Reference Portraits'),
      el('div', { class: 'subgrid' },
        ...Object.entries(c.references).map(([fac, ref]) => el('div', { class: 'minicard ' + factionClass(fac) },
          el('div', { class: 'mc-tag' }, `${factionLabel(fac)} · ${pretty(ref.race)}`),
          el('div', { class: 'portrait-pair', style: 'margin-top:6px' },
            ref.male ? el('div', { class: 'p' }, el('img', { src: `characters/${ref.stem}.male.png`, alt: '' })) : null,
            ref.female ? el('div', { class: 'p' }, el('img', { src: `characters/${ref.stem}.female.png`, alt: '' })) : null,
          ),
        )),
      ),
    ));
  }
}

function renderOrderPage(id) {
  const o = (state.data.orders || []).find((x) => x.id === id);
  const body = $('#page-body'); clearChildren(body);
  if (!o) { body.appendChild(el('div', null, 'Order not found.')); return; }
  $('#page-back').href = '#institutions';

  body.appendChild(el('div', { class: 'page-hero' },
    el('div', { class: 'hero-text' },
      el('h1', { class: 'page-title' }, o.name || pretty(o.id)),
      el('div', { class: 'page-subtitle' }, `Order · class ${o.class_id ?? '?'}`),
      o.flavor ? el('p', { class: 'page-lede' }, o.flavor) : null,
    ),
  ));

  const skip = new Set(['id', 'name', 'flavor', '_file', 'portraits', 'class_id', 'specs']);
  const entries = Object.entries(o).filter(([k]) => !skip.has(k));
  if (entries.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, 'Details'),
      el('dl', null,
        ...entries.flatMap(([k, v]) => [
          el('dt', null, pretty(k)),
          el('dd', null, typeof v === 'object' ? JSON.stringify(v, null, 2) : String(v)),
        ]),
      ),
    ));
  }

  if (o.specs?.length) {
    body.appendChild(el('div', { class: 'page-section' },
      el('h3', null, `Specs (${o.specs.length})`),
      el('div', { class: 'subgrid' },
        ...o.specs.map((s) => el('div', { class: 'minicard' },
          el('div', { class: 'mc-title' }, s.name || pretty(s.id || '')),
          s.description ? el('div', { class: 'mc-meta' }, s.description) : null,
        )),
      ),
    ));
  }
}

// ─── SCHOOLS ────────────────────────────────────────────────────────────
function renderSchools() {
  const grid = $('#school-grid');
  clearChildren(grid);
  for (const s of state.data.schools || []) {
    const card = el('div', { class: 'card pillar-' + (s.pillar || 'arcana') + (s.morality ? ' morality-' + s.morality : '') },
      s.has_emblem ? el('div', { class: 'emblem' },
        el('img', { src: `emblems/school_${s.name}.png`, alt: '' })) : null,
      el('div', { class: 'card-title' }, pretty(s.name)),
      el('div', { class: 'card-subtitle' }, `${pretty(s.pillar || '')} · ${pretty(s.morality || '')}`),
      el('div', { class: 'card-desc' }, s.tag || s.icon_style?.motif || ''),
      el('div', { class: 'card-meta' },
        s.damage_type ? chip(pretty(s.damage_type)) : null,
        s.family ? chip(pretty(s.family)) : null,
      ),
    );
    card.addEventListener('click', () => openSchoolDetail(s));
    grid.appendChild(card);
  }
}

function openSchoolDetail(s) {
  const node = el('div', null,
    el('h2', null, pretty(s.name)),
    el('div', { class: 'meta' }, `${pretty(s.pillar || '')} · ${pretty(s.morality || '')}`),
    s.has_emblem ? el('img', { class: 'detail-img', src: `emblems/school_${s.name}.png`, alt: '' }) : null,
    el('dl', null,
      s.family ? el('dt', null, 'Family') : null,
      s.family ? el('dd', null, pretty(s.family)) : null,
      s.tag ? el('dt', null, 'Tag') : null,
      s.tag ? el('dd', null, s.tag) : null,
      s.damage_type ? el('dt', null, 'Damage type') : null,
      s.damage_type ? el('dd', null, pretty(s.damage_type)) : null,
      s.applies_to_categories?.length ? el('dt', null, 'Applies to') : null,
      s.applies_to_categories?.length ? el('dd', null, s.applies_to_categories.map(pretty).join(' · ')) : null,
      s.icon_style?.palette ? el('dt', null, 'Palette') : null,
      s.icon_style?.palette ? el('dd', null, s.icon_style.palette) : null,
      s.icon_style?.motif ? el('dt', null, 'Motif') : null,
      s.icon_style?.motif ? el('dd', null, s.icon_style.motif) : null,
    ),
  );
  openOverlay(node);
}

// ─── SPELLS ─────────────────────────────────────────────────────────────
function populateSpellFilters() {
  const sp = state.data.spells || [];
  fillSelect($('#spell-pillar'),   uniqueSorted(sp.map((s) => s.pillar)));
  fillSelect($('#spell-category'), uniqueSorted(sp.map((s) => s.category)));
  fillSelect($('#spell-school'),   uniqueSorted(sp.map((s) => s.school)));
  fillSelect($('#spell-tier'),     uniqueSorted(sp.map((s) => s.tier)).map(String));
  fillSelect($('#spell-morality'), uniqueSorted(sp.map((s) => s.morality).filter(Boolean)));
}

function wireSpellFilters() {
  const f = state.filters.spell;
  $('#spell-search').addEventListener('input',  (e) => { f.search   = e.target.value.toLowerCase(); renderSpells(); });
  $('#spell-pillar').addEventListener('change', (e) => { f.pillar   = e.target.value; renderSpells(); });
  $('#spell-category').addEventListener('change', (e) => { f.category = e.target.value; renderSpells(); });
  $('#spell-school').addEventListener('change', (e) => { f.school   = e.target.value; renderSpells(); });
  $('#spell-tier').addEventListener('change',   (e) => { f.tier     = e.target.value; renderSpells(); });
  $('#spell-morality').addEventListener('change', (e) => { f.morality = e.target.value; renderSpells(); });
  $('#spell-hasicon').addEventListener('change',(e) => { f.hasIcon  = e.target.checked; renderSpells(); });
}

function renderSpells() {
  const grid = $('#spell-grid'); clearChildren(grid);
  const f = state.filters.spell;
  let count = 0;
  let rendered = 0;
  for (const s of state.data.spells || []) {
    if (f.pillar && s.pillar !== f.pillar) continue;
    if (f.category && s.category !== f.category) continue;
    if (f.school && s.school !== f.school) continue;
    if (f.tier && String(s.tier) !== f.tier) continue;
    if (f.morality && s.morality !== f.morality) continue;
    if (f.hasIcon && !s.has_icon) continue;
    if (f.search) {
      const q = ((s.display_name || '') + ' ' + (s.description || '')).toLowerCase();
      if (!q.includes(f.search)) continue;
    }
    count++;
    if (rendered >= 600) continue; // hard cap to keep the grid responsive
    rendered++;
    const card = el('div', { class: 'card spell-card pillar-' + s.pillar + (s.morality ? ' morality-' + s.morality : '') },
      el('div', { class: 'icon' },
        s.has_icon ? el('img', { src: `icons/${s.id}.png`, alt: '' }) : 'no icon'),
      el('div', { class: 'card-title' }, s.display_name),
      el('div', { class: 'card-subtitle' }, `${pretty(s.pillar)} · ${pretty(s.school)}`),
      el('div', { class: 'card-desc' }, s.description || ''),
      el('div', { class: 'card-meta' },
        el('span', { class: `tier-badge tier-${s.tier}` }, `T${s.tier}`),
        chip(pretty(s.category)),
        s.damage_type ? chip(pretty(s.damage_type)) : null,
      ),
    );
    card.addEventListener('click', () => openSpellDetail(s));
    grid.appendChild(card);
  }
  $('#spell-count').textContent = `${count} of ${(state.data.spells || []).length}` +
    (count > rendered ? ` (showing first ${rendered})` : '');
}

function openSpellDetail(s) {
  const node = el('div', null,
    el('h2', null, s.display_name),
    el('div', { class: 'meta' }, `${pretty(s.pillar)} · ${pretty(s.category)} · ${pretty(s.school)} · T${s.tier}`),
    s.has_icon ? el('img', { class: 'detail-img', src: `icons/${s.id}.png`, alt: '' }) : null,
    el('dl', null,
      el('dt', null, 'Description'),
      el('dd', null, s.description || '—'),
      s.damage_type ? el('dt', null, 'Damage type') : null,
      s.damage_type ? el('dd', null, pretty(s.damage_type)) : null,
      s.morality ? el('dt', null, 'Morality') : null,
      s.morality ? el('dd', { class: `morality-${s.morality}` }, pretty(s.morality)) : null,
      el('dt', null, 'Internal id'),
      el('dd', { style: 'font-family:ui-monospace,monospace; font-size:11px; color:var(--parchment-faint)' }, s.id),
    ),
  );
  openOverlay(node);
}

// ─── route table ────────────────────────────────────────────────────────
const ENTITY_PAGES = {
  race:        renderRacePage,
  class:       renderClassPage,
  order:       renderOrderPage,
  institution: renderInstitutionPage,
  zone:        renderZonePage,
  hub:         renderHubPage,
  landmark:    renderLandmarkPage,
  biome:       renderBiomePage,
  dungeon:     renderDungeonPage,
  faction:     renderFactionPage,
};

document.addEventListener('DOMContentLoaded', load);
