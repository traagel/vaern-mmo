'use strict';

const state = {
  data: null,
  filters: { search: '', pillar: '', category: '', school: '', tier: '', morality: '', hasIcon: false },
};

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

async function load() {
  const r = await fetch('data.json');
  state.data = await r.json();
  renderStats();
  populateSpellFilters();
  wireFilters();
  wireTabs();
  wireOverlay();
  renderSpells();
  renderSchools();
  renderClasses();
  renderRaces();
  renderInstitutions();
  renderFactions();
  window.addEventListener('hashchange', route);
  route();
}

function route() {
  const hash = window.location.hash.replace(/^#/, '');
  const [kind, id] = hash.split('/');
  const pageEl = $('#page');
  if (kind === 'race' && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    renderRacePage(id);
    window.scrollTo(0, 0);
    return;
  }
  if (kind === 'combo' && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    renderComboPage(id);
    window.scrollTo(0, 0);
    return;
  }
  if (kind === 'class' && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    renderClassPage(parseInt(id, 10));
    window.scrollTo(0, 0);
    return;
  }
  if (kind === 'order' && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    renderOrderPage(id);
    window.scrollTo(0, 0);
    return;
  }
  if (kind === 'institution' && id) {
    document.body.classList.add('page-mode');
    pageEl.classList.remove('hidden');
    renderInstitutionPage(id);
    window.scrollTo(0, 0);
    return;
  }
  document.body.classList.remove('page-mode');
  pageEl.classList.add('hidden');
  // restore the correct tab based on hash
  const tabHashes = ['spells', 'schools', 'classes', 'races', 'institutions', 'factions'];
  const wanted = tabHashes.includes(kind) ? kind : (state.currentTab || 'spells');
  activateTab(wanted);
}

function activateTab(name) {
  $$('nav#tabs button').forEach((x) => x.classList.toggle('active', x.dataset.tab === name));
  $$('main section.tab').forEach((t) => t.classList.toggle('active', t.id === `${name}-tab`));
  state.currentTab = name;
}

function renderStats() {
  const c = state.data.counts;
  $('#stats').textContent =
    `${c.spells_total} spells · ${c.spells_with_icon} icons · ` +
    `${state.data.races.length} races · ${state.data.classes.length} archetypes · ` +
    `${(state.data.orders || []).length} orders · ` +
    `${(state.data.institutions || []).length} institutions · ` +
    `${state.data.schools.length} schools · ${c.portraits} portraits · ${c.emblems} emblems · ` +
    `data ${state.data.generated_at}`;
}

function wireTabs() {
  $$('nav#tabs button').forEach((b) => {
    b.addEventListener('click', () => {
      window.location.hash = b.dataset.tab;
    });
  });
}

function wireOverlay() {
  $('#detail-close').addEventListener('click', () => $('#detail-overlay').classList.add('hidden'));
  $('#detail-overlay').addEventListener('click', (e) => {
    if (e.target.id === 'detail-overlay') $('#detail-overlay').classList.add('hidden');
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') $('#detail-overlay').classList.add('hidden');
  });
}

function showDetail(title, meta, fields, imgUrl) {
  const body = $('#detail-body');
  body.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = title;
  body.appendChild(h);
  if (meta) {
    const m = document.createElement('div');
    m.className = 'meta';
    m.textContent = meta;
    body.appendChild(m);
  }
  if (imgUrl) {
    const img = document.createElement('img');
    img.className = 'detail-img';
    img.src = imgUrl;
    body.appendChild(img);
  }
  const dl = document.createElement('dl');
  for (const [k, v] of fields) {
    if (v == null || v === '') continue;
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    if (typeof v === 'object') {
      dd.innerHTML = `<pre style="margin:0;white-space:pre-wrap;font:11px ui-monospace,monospace;color:var(--text-dim)">${escape(JSON.stringify(v, null, 2))}</pre>`;
    } else {
      dd.textContent = String(v);
    }
    dl.appendChild(dt);
    dl.appendChild(dd);
  }
  body.appendChild(dl);
  $('#detail-overlay').classList.remove('hidden');
}

function escape(s) {
  return String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
}

function archetypeChip(cls, extraText) {
  const link = document.createElement('a');
  link.href = `#class/${cls.class_id}`;
  const doms = cls.dominant_pillar || [];
  const color = (p) => `var(--${p})`;
  let border;
  if (doms.length === 1) {
    border = `3px solid ${color(doms[0])}`;
  } else if (doms.length >= 2) {
    // stacked indicators for hybrid — double left-border via box-shadow
    link.style.boxShadow = `-3px 0 0 ${color(doms[0])}, -6px 0 0 ${color(doms[1])}`;
    link.style.marginLeft = '6px';
    border = '0';
  } else {
    border = '3px solid var(--text-dim)';
  }
  const pos = cls.position;
  const text = extraText || `${cls.internal_label} (${pos.might}/${pos.arcana}/${pos.finesse})`;
  link.textContent = text;
  link.style.cssText +=
    `;text-decoration:none;cursor:pointer;padding:4px 10px 4px 8px;font-size:12px;` +
    `background:var(--surface);border:1px solid var(--border);border-radius:4px;` +
    `color:var(--text);display:inline-block;` +
    (border !== '0' ? `border-left:${border};` : '');
  return link;
}

// --- spells ---

function populateSpellFilters() {
  const s = state.data.spells;
  const uniq = (key) => [...new Set(s.map((x) => x[key]))].sort((a, b) => String(a).localeCompare(String(b)));
  fill('#spell-pillar', uniq('pillar'));
  fill('#spell-category', uniq('category'));
  fill('#spell-school', uniq('school'));
  fill('#spell-tier', uniq('tier'));
  fill('#spell-morality', uniq('morality').filter(Boolean));
}

function fill(sel, values) {
  const el = $(sel);
  for (const v of values) {
    const opt = document.createElement('option');
    opt.value = v;
    opt.textContent = v;
    el.appendChild(opt);
  }
}

function wireFilters() {
  const map = {
    '#spell-search': 'search',
    '#spell-pillar': 'pillar',
    '#spell-category': 'category',
    '#spell-school': 'school',
    '#spell-tier': 'tier',
    '#spell-morality': 'morality',
  };
  for (const [sel, key] of Object.entries(map)) {
    $(sel).addEventListener('input', (e) => {
      state.filters[key] = e.target.value;
      renderSpells();
    });
  }
  $('#spell-hasicon').addEventListener('change', (e) => {
    state.filters.hasIcon = e.target.checked;
    renderSpells();
  });
}

function filteredSpells() {
  const f = state.filters;
  return state.data.spells.filter((s) => {
    if (f.pillar && s.pillar !== f.pillar) return false;
    if (f.category && s.category !== f.category) return false;
    if (f.school && s.school !== f.school) return false;
    if (f.tier && String(s.tier) !== String(f.tier)) return false;
    if (f.morality && s.morality !== f.morality) return false;
    if (f.hasIcon && !s.has_icon) return false;
    if (f.search) {
      const q = f.search.toLowerCase();
      if (!s.display_name.toLowerCase().includes(q) &&
          !s.description.toLowerCase().includes(q) &&
          !s.id.toLowerCase().includes(q)) return false;
    }
    return true;
  });
}

function renderSpells() {
  const list = filteredSpells();
  $('#spell-count').textContent = `${list.length} / ${state.data.spells.length}`;
  const g = $('#spell-grid');
  g.innerHTML = '';
  for (const s of list.slice(0, 300)) {
    g.appendChild(spellCard(s));
  }
  if (list.length > 300) {
    const note = document.createElement('div');
    note.style.cssText = 'grid-column: 1/-1; color: var(--text-faint); padding: 12px; text-align: center; font-size: 12px';
    note.textContent = `showing first 300 of ${list.length} — narrow filters to see more`;
    g.appendChild(note);
  }
}

function spellCard(s) {
  const c = document.createElement('div');
  c.className = `card spell-card pillar-${s.pillar}`;
  const icon = document.createElement('div');
  icon.className = 'icon';
  if (s.has_icon) {
    const img = document.createElement('img');
    img.src = `../icons/${s.id}.png`;
    img.loading = 'lazy';
    icon.appendChild(img);
  } else {
    icon.textContent = 'no icon';
  }
  c.appendChild(icon);

  const title = document.createElement('div');
  title.className = 'card-title';
  title.textContent = s.display_name;
  c.appendChild(title);

  const desc = document.createElement('div');
  desc.className = 'card-desc';
  desc.textContent = s.description;
  c.appendChild(desc);

  const meta = document.createElement('div');
  meta.className = 'card-meta';
  meta.innerHTML =
    `<span class="tier-badge tier-${s.tier}">T${s.tier}</span>` +
    `<span class="chip">${s.school}</span>` +
    `<span class="chip">${s.category}</span>` +
    (s.morality ? `<span class="chip morality-${s.morality}">${s.morality}</span>` : '') +
    (s.damage_type ? `<span class="chip">${s.damage_type}</span>` : '');
  c.appendChild(meta);

  c.addEventListener('click', () =>
    showDetail(
      s.display_name,
      s.id,
      [
        ['pillar', s.pillar],
        ['category', s.category],
        ['school', s.school],
        ['tier', s.tier],
        ['morality', s.morality],
        ['damage type', s.damage_type],
        ['description', s.description],
      ],
      s.has_icon ? `../icons/${s.id}.png` : null,
    ),
  );
  return c;
}

// --- schools ---

function renderSchools() {
  const g = $('#school-grid');
  g.innerHTML = '';
  for (const s of state.data.schools) {
    const c = document.createElement('div');
    c.className = `card pillar-${s.pillar}`;
    if (s.has_emblem) {
      const emb = document.createElement('div');
      emb.className = 'emblem';
      const img = document.createElement('img');
      img.src = `../emblems/school_${s.name}.png`;
      img.loading = 'lazy';
      emb.appendChild(img);
      c.appendChild(emb);
    }
    const body = document.createElement('div');
    body.innerHTML =
      `<div class="card-title">${s.name}</div>` +
      `<div class="card-desc">${s.tag || ''} · ${s.family || ''}</div>` +
      `<div class="card-meta">` +
      `<span class="chip">${s.pillar}</span>` +
      `<span class="chip morality-${s.morality}">${s.morality}</span>` +
      (s.damage_type ? `<span class="chip">${s.damage_type}</span>` : '') +
      `</div>`;
    c.appendChild(body);
    if (s.icon_style) {
      const sty = document.createElement('div');
      sty.style.cssText = 'font-size:11px;color:var(--text-faint);line-height:1.4';
      sty.textContent = s.icon_style.motif || '';
      c.appendChild(sty);
    }
    c.addEventListener('click', () =>
      showDetail(
        s.name,
        `${s.pillar} · ${s.morality}`,
        [
          ['family', s.family],
          ['tag', s.tag],
          ['damage type', s.damage_type],
          ['applies to', (s.applies_to_categories || []).join(', ')],
          ['icon palette', s.icon_style?.palette],
          ['icon motif', s.icon_style?.motif],
          ['icon silhouette', s.icon_style?.silhouette],
          ['icon material', s.icon_style?.material],
          ['file', s._file],
        ],
        s.has_emblem ? `../emblems/school_${s.name}.png` : null,
      ),
    );
    g.appendChild(c);
  }
}

// --- classes ---

function classPortraitUrl(cls, gender, faction) {
  faction = faction || 'faction_a';
  const ref = cls.references?.[faction];
  if (!ref || !ref[gender]) return null;
  return `../characters/${ref.stem}.${gender}.png`;
}

function classAnyPortrait(cls) {
  for (const f of ['faction_a', 'faction_b']) {
    for (const g of ['male', 'female']) {
      const u = classPortraitUrl(cls, g, f);
      if (u) return u;
    }
  }
  return null;
}

function renderClasses() {
  const g = $('#class-grid');
  g.innerHTML = '';
  for (const cls of state.data.classes) {
    const c = document.createElement('div');
    const dominant = (cls.dominant_pillar || [])[0] || 'arcana';
    c.className = `card class-card pillar-${dominant}`;
    const pos = cls.position;

    // portrait thumb — Concord + Rend side-by-side
    const portraits = document.createElement('div');
    portraits.className = 'portrait-pair';
    for (const faction of ['faction_a', 'faction_b']) {
      const p = document.createElement('div');
      p.className = 'p';
      const url = classPortraitUrl(cls, 'male', faction) || classPortraitUrl(cls, 'female', faction);
      if (url) {
        const img = document.createElement('img');
        img.src = url;
        img.loading = 'lazy';
        p.appendChild(img);
      } else {
        p.textContent = faction === 'faction_a' ? 'A' : 'B';
      }
      portraits.appendChild(p);
    }
    c.appendChild(portraits);

    const body = document.createElement('div');
    body.innerHTML =
      `<div class="card-title">${cls.abstract_name || cls.internal_label}` +
        (cls.abstract_name ? ` <span style="color:var(--text-faint);font-weight:400;font-size:12px">(${cls.internal_label})</span>` : '') +
      `</div>` +
      `<div class="card-desc">${cls.note || ''}</div>` +
      `<div class="pos">` +
      barRow('Mig', pos.might, '--might') +
      barRow('Arc', pos.arcana, '--arcana') +
      barRow('Fin', pos.finesse, '--finesse') +
      `</div>` +
      `<div class="card-meta">` +
      (cls.primary_roles || []).map((r) => `<span class="chip">${r}</span>`).join('') +
      (cls.portraits?.male ? '<span class="chip">♂</span>' : '') +
      (cls.portraits?.female ? '<span class="chip">♀</span>' : '') +
      `</div>`;
    c.appendChild(body);

    c.addEventListener('click', () => { window.location.hash = `class/${cls.class_id}`; });
    g.appendChild(c);
  }
}

function renderClassPage(classId) {
  const cls = state.data.classes.find((k) => k.class_id === classId);
  const body = $('#page-body');
  body.innerHTML = '';
  if (!cls) {
    body.textContent = `class ${classId} not found`;
    return;
  }
  $('#page-back').href = '#classes';

  const title = document.createElement('h1');
  title.className = 'page-title';
  title.textContent = cls.abstract_name || cls.internal_label;
  body.appendChild(title);

  const pos = cls.position;
  const sub = document.createElement('div');
  sub.className = 'page-subtitle';
  const labelBit = cls.abstract_name ? `flagship class: ${cls.internal_label} · ` : '';
  sub.textContent = `${labelBit}position M${pos.might} / A${pos.arcana} / F${pos.finesse} · ${cls.pillar_classification} · ${(cls.primary_roles || []).join(', ')}`;
  body.appendChild(sub);

  const hero = document.createElement('div');
  hero.className = 'page-hero';
  for (const faction of ['faction_a', 'faction_b']) {
    for (const gender of ['male', 'female']) {
      const slot = document.createElement('div');
      slot.className = 'portrait-slot';
      const url = classPortraitUrl(cls, gender, faction);
      if (url) {
        const img = document.createElement('img');
        img.src = url;
        slot.appendChild(img);
      } else {
        slot.textContent = `no ${gender} · ${faction}`;
      }
      const lbl = document.createElement('div');
      lbl.className = 'label';
      const race = cls.references?.[faction]?.race || '?';
      const factionShort = faction === 'faction_a' ? 'Concord' : 'Rend';
      lbl.textContent = `${gender} · ${race} / ${factionShort}`;
      slot.appendChild(lbl);
      hero.appendChild(slot);
    }
  }
  body.appendChild(hero);

  body.appendChild(sectionDl('Kit', [
    ['pitch', cls.visual?.pitch],
    ['armor', cls.visual?.armor],
    ['weapon', cls.visual?.weapon],
    ['pose', cls.visual?.pose],
    ['negative tags', cls.visual?.negative_tags],
  ]));
  body.appendChild(sectionDl('Position', [
    ['edge', cls.edge],
    ['dominant pillar', (cls.dominant_pillar || []).join(', ')],
    ['primary roles', (cls.primary_roles || []).join(', ')],
    ['note', cls.note],
  ]));
  body.appendChild(sectionDl('Capabilities', [
    ['might', (cls.capabilities?.might || []).join(' · ')],
    ['arcana', (cls.capabilities?.arcana || []).join(' · ')],
    ['finesse', (cls.capabilities?.finesse || []).join(' · ')],
  ]));

  // Orders at this archetype — Concord + Rend
  const archOrders = (state.data.orders || []).filter((o) => o.archetype_id === classId);
  if (archOrders.length) {
    archOrders.sort((a, b) => a.faction.localeCompare(b.faction));
    const sec = document.createElement('div');
    sec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Orders at this Archetype';
    sec.appendChild(h);
    const note = document.createElement('div');
    note.style.cssText = 'color:var(--text-dim);font-size:12px;margin-bottom:10px';
    note.textContent = `${archOrders.length} Order${archOrders.length === 1 ? '' : 's'} sit at this position`;
    sec.appendChild(note);
    const grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill, minmax(240px, 1fr));gap:12px';
    for (const o of archOrders) {
      const card = document.createElement('a');
      card.href = `#order/${o.id}`;
      card.style.cssText =
        'text-decoration:none;color:var(--text);background:var(--surface);' +
        'border:1px solid var(--border);border-radius:6px;overflow:hidden;' +
        'display:flex;flex-direction:column;gap:6px;padding:8px;transition:border-color 0.1s';
      card.addEventListener('mouseenter', () => (card.style.borderColor = 'var(--arcana)'));
      card.addEventListener('mouseleave', () => (card.style.borderColor = 'var(--border)'));
      const pair = document.createElement('div');
      pair.className = 'portrait-pair';
      for (const gender of ['male', 'female']) {
        const p = document.createElement('div');
        p.className = 'p';
        if (o.portraits?.[gender]) {
          const img = document.createElement('img');
          img.src = `../characters/${o.id}.${gender}.png`;
          img.loading = 'lazy';
          p.appendChild(img);
        } else {
          p.textContent = gender[0];
        }
        pair.appendChild(p);
      }
      card.appendChild(pair);
      const label = document.createElement('div');
      label.style.cssText = 'font-size:13px;font-weight:600';
      const factionShort = o.faction === 'faction_a' ? 'Concord' : 'Rend';
      label.textContent = `${o.player_facing?.class_name || o.id} · ${factionShort}`;
      card.appendChild(label);
      if (o.aesthetic?.pitch) {
        const desc = document.createElement('div');
        desc.style.cssText = 'font-size:11px;color:var(--text-dim);line-height:1.4';
        desc.textContent = o.aesthetic.pitch;
        card.appendChild(desc);
      }
      grid.appendChild(card);
    }
    sec.appendChild(grid);
    body.appendChild(sec);
  }

  body.appendChild(sectionDl('Metadata', [
    ['file', cls._file],
  ]));
}

function barRow(label, pct, varname) {
  return `<div><span style="display:inline-block;width:28px;color:var(--text-faint)">${label}</span>` +
    `<span class="bar" style="width:${pct}px;background:var(${varname});opacity:${pct === 0 ? 0.15 : 1}"></span>` +
    `<span style="margin-left:6px">${pct}</span></div>`;
}

// --- races ---

function racePortraitUrl(r, gender) {
  const p = r.portraits || {};
  if (gender === 'male' && p.male) return `../characters/${r.id}.male.png`;
  if (gender === 'female' && p.female) return `../characters/${r.id}.female.png`;
  if (p.male) return `../characters/${r.id}.male.png`;
  if (p.female) return `../characters/${r.id}.female.png`;
  if (p.neutral) return `../characters/${r.id}.png`;
  return null;
}

function renderRaces() {
  const g = $('#race-grid');
  g.innerHTML = '';
  for (const r of state.data.races) {
    const c = document.createElement('div');
    c.className = `card race-card ${r.faction}`;
    const portrait = document.createElement('div');
    portrait.className = 'portrait';
    const url = racePortraitUrl(r, 'male');
    if (url) {
      const img = document.createElement('img');
      img.src = url;
      img.loading = 'lazy';
      portrait.appendChild(img);
    } else {
      portrait.textContent = 'no portrait';
    }
    c.appendChild(portrait);
    const title = document.createElement('div');
    title.className = 'card-title';
    title.textContent = r.id.replace(/_/g, ' ');
    c.appendChild(title);
    const desc = document.createElement('div');
    desc.className = 'card-desc';
    desc.textContent = r.archetype || '';
    c.appendChild(desc);
    const meta = document.createElement('div');
    meta.className = 'card-meta';
    meta.innerHTML =
      `<span class="chip">${r.faction}</span>` +
      `<span class="chip">${r.favored_class || ''}</span>` +
      (r.portraits?.male ? '<span class="chip">♂</span>' : '') +
      (r.portraits?.female ? '<span class="chip">♀</span>' : '');
    c.appendChild(meta);
    c.addEventListener('click', () => { window.location.hash = `race/${r.id}`; });
    g.appendChild(c);
  }
}

function renderCombos() {
  const g = $('#combo-grid');
  if (!g) return;
  g.innerHTML = '';
  for (const c of state.data.combos || []) {
    const card = document.createElement('div');
    const cls = state.data.classes.find((k) => k.class_id === c.class_id);
    const dominant = cls?.dominant_pillar?.[0] || 'arcana';
    card.className = `card combo-card ${c.faction} pillar-${dominant}`;
    const pair = document.createElement('div');
    pair.className = 'portrait-pair';
    for (const gender of ['male', 'female']) {
      const slot = document.createElement('div');
      slot.className = 'p';
      if (c.portraits?.[gender]) {
        const img = document.createElement('img');
        img.src = `../characters/${c._stem}.${gender}.png`;
        img.loading = 'lazy';
        slot.appendChild(img);
      } else {
        slot.textContent = gender[0];
      }
      pair.appendChild(slot);
    }
    card.appendChild(pair);

    const title = document.createElement('div');
    title.className = 'card-title';
    title.textContent = `${c.race.replace(/_/g, ' ')} · ${c.class_label}`;
    card.appendChild(title);

    const desc = document.createElement('div');
    desc.className = 'card-desc';
    desc.textContent = c.notes || '';
    card.appendChild(desc);

    const meta = document.createElement('div');
    meta.className = 'card-meta';
    meta.innerHTML =
      `<span class="chip">${c.faction}</span>` +
      `<span class="tier-badge">class ${c.class_id}</span>`;
    card.appendChild(meta);

    card.addEventListener('click', () => { window.location.hash = `combo/${c.id}`; });
    g.appendChild(card);
  }
}

function renderRacePage(raceId) {
  const r = state.data.races.find((x) => x.id === raceId);
  const body = $('#page-body');
  body.innerHTML = '';
  if (!r) {
    body.textContent = `race ${raceId} not found`;
    return;
  }
  $('#page-back').href = '#races';

  // hero — title + portraits
  const title = document.createElement('h1');
  title.className = 'page-title';
  title.textContent = r.id.replace(/_/g, ' ');
  body.appendChild(title);

  const sub = document.createElement('div');
  sub.className = 'page-subtitle';
  sub.textContent = `${r.archetype || ''} · ${r.faction}`;
  body.appendChild(sub);

  const hero = document.createElement('div');
  hero.className = 'page-hero';
  for (const gender of ['male', 'female']) {
    const slot = document.createElement('div');
    slot.className = 'portrait-slot';
    if (r.portraits?.[gender]) {
      const img = document.createElement('img');
      img.src = `../characters/${r.id}.${gender}.png`;
      slot.appendChild(img);
    } else {
      slot.textContent = `no ${gender} portrait`;
    }
    const lbl = document.createElement('div');
    lbl.className = 'label';
    lbl.textContent = gender;
    slot.appendChild(lbl);
    hero.appendChild(slot);
  }
  body.appendChild(hero);

  // sections
  body.appendChild(sectionDl('Overview', [
    ['favored class', r.favored_class],
    ['cultural traits', r.cultural_traits],
  ]));

  // Affinity + reachable archetypes
  const aff = r.affinity;
  if (aff) {
    const sec = document.createElement('div');
    sec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Affinity & Reachable Archetypes';
    sec.appendChild(h);
    const affLine = document.createElement('div');
    affLine.style.cssText = 'font-size:13px;margin-bottom:10px';
    affLine.innerHTML =
      `<span style="color:var(--might)">Might ${aff.might}</span> · ` +
      `<span style="color:var(--arcana)">Arcana ${aff.arcana}</span> · ` +
      `<span style="color:var(--finesse)">Finesse ${aff.finesse}</span>` +
      (aff.notes ? `<div style="color:var(--text-dim);margin-top:4px;font-size:12px">${aff.notes}</div>` : '');
    sec.appendChild(affLine);
    const reachable = state.data.classes.filter((c) =>
      c.position.might <= aff.might &&
      c.position.arcana <= aff.arcana &&
      c.position.finesse <= aff.finesse,
    );
    const reachList = document.createElement('div');
    reachList.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;margin-top:8px';
    for (const c of reachable) reachList.appendChild(archetypeChip(c));
    const count = document.createElement('div');
    count.style.cssText = 'color:var(--text-dim);font-size:12px;margin-top:10px';
    count.textContent = `${reachable.length} of ${state.data.classes.length} archetypes reachable`;
    sec.appendChild(reachList);
    sec.appendChild(count);
    body.appendChild(sec);

    // Orders this race could join: same faction + archetype reachable
    const reachableIds = new Set(reachable.map((c) => c.class_id));
    const orders = (state.data.orders || []).filter(
      (o) => o.faction === r.faction && reachableIds.has(o.archetype_id),
    );
    if (orders.length) {
      const osec = document.createElement('div');
      osec.className = 'page-section';
      const oh = document.createElement('h3');
      oh.textContent = 'Orders This Race Could Join';
      osec.appendChild(oh);
      const note = document.createElement('div');
      note.style.cssText = 'color:var(--text-dim);font-size:12px;margin-bottom:10px';
      note.textContent = `${orders.length} Order${orders.length === 1 ? '' : 's'} available — same faction, archetype within affinity caps`;
      osec.appendChild(note);
      const grid = document.createElement('div');
      grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill, minmax(200px, 1fr));gap:8px';
      for (const o of orders) {
        const cls = state.data.classes.find((c) => c.class_id === o.archetype_id);
        const a = document.createElement('a');
        a.href = `#order/${o.id}`;
        a.style.cssText =
          'text-decoration:none;background:var(--surface);border:1px solid var(--border);' +
          'padding:8px 10px;border-radius:4px;display:block;color:var(--text);font-size:13px;' +
          'transition:border-color 0.1s';
        a.addEventListener('mouseenter', () => (a.style.borderColor = 'var(--arcana)'));
        a.addEventListener('mouseleave', () => (a.style.borderColor = 'var(--border)'));
        a.innerHTML =
          `<div style="font-weight:600">${o.player_facing?.class_name || o.id}</div>` +
          `<div style="color:var(--text-dim);font-size:11px;margin-top:2px">${cls?.internal_label || 'archetype ' + o.archetype_id}</div>`;
        grid.appendChild(a);
      }
      osec.appendChild(grid);
      body.appendChild(osec);
    }
  }

  // Flagship combos for this race
  const flagships = (state.data.combos || []).filter((c) => c.race === r.id);
  if (flagships.length) {
    const sec = document.createElement('div');
    sec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Flagship Build' + (flagships.length > 1 ? 's' : '');
    sec.appendChild(h);
    const grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill, minmax(260px, 1fr));gap:12px';
    for (const c of flagships) {
      const cls = state.data.classes.find((k) => k.class_id === c.class_id);
      const card = document.createElement('a');
      card.href = `#combo/${c.id}`;
      card.style.cssText =
        'text-decoration:none;color:var(--text);background:var(--surface);' +
        'border:1px solid var(--border);border-radius:6px;overflow:hidden;' +
        'display:flex;flex-direction:column;gap:6px;padding:8px;transition:border-color 0.1s';
      card.addEventListener('mouseenter', () => (card.style.borderColor = 'var(--arcana)'));
      card.addEventListener('mouseleave', () => (card.style.borderColor = 'var(--border)'));
      const pair = document.createElement('div');
      pair.className = 'portrait-pair';
      for (const gender of ['male', 'female']) {
        const p = document.createElement('div');
        p.className = 'p';
        if (c.portraits?.[gender]) {
          const img = document.createElement('img');
          img.src = `../characters/${c._stem}.${gender}.png`;
          img.loading = 'lazy';
          p.appendChild(img);
        } else {
          p.textContent = gender[0];
        }
        pair.appendChild(p);
      }
      card.appendChild(pair);
      const label = document.createElement('div');
      label.style.cssText = 'font-size:13px;font-weight:600';
      label.textContent = `${c.race.replace(/_/g, ' ')} · ${c.class_label}`;
      card.appendChild(label);
      if (c.notes) {
        const notes = document.createElement('div');
        notes.style.cssText = 'font-size:11px;color:var(--text-dim);line-height:1.4';
        notes.textContent = c.notes;
        card.appendChild(notes);
      }
      grid.appendChild(card);
    }
    sec.appendChild(grid);
    body.appendChild(sec);
  }

  body.appendChild(sectionDl('Prompt Anchors', [
    ['pitch', r.visual?.pitch],
    ['signature', r.visual?.signature],
    ['tags', r.visual?.tags],
    ['negative tags', r.visual?.negative_tags],
  ]));
  body.appendChild(sectionDl('Visual Detail', [
    ['body', r.visual?.body],
    ['features', r.visual?.features],
    ['hair', r.visual?.hair_style],
    ['silhouette', r.visual?.silhouette],
    ['attire', r.visual?.attire_baseline],
    ['distinguishing marks', r.visual?.distinguishing_marks],
  ]));
  body.appendChild(sectionDl('Metadata', [
    ['file', r._file],
  ]));
}

function renderComboPage(comboId) {
  const c = (state.data.combos || []).find((x) => x.id === comboId);
  const body = $('#page-body');
  body.innerHTML = '';
  if (!c) {
    body.textContent = `combo ${comboId} not found`;
    return;
  }
  $('#page-back').href = '#combos';

  const race = state.data.races.find((r) => r.id === c.race);
  const cls = state.data.classes.find((k) => k.class_id === c.class_id);
  const faction = state.data.factions.find((f) => f.id === c.faction);

  const title = document.createElement('h1');
  title.className = 'page-title';
  title.textContent = `${c.race.replace(/_/g, ' ')} · ${c.class_label}`;
  body.appendChild(title);

  const sub = document.createElement('div');
  sub.className = 'page-subtitle';
  sub.textContent = `${c.faction} · ${c.notes || ''}`;
  body.appendChild(sub);

  const hero = document.createElement('div');
  hero.className = 'page-hero';
  for (const gender of ['male', 'female']) {
    const slot = document.createElement('div');
    slot.className = 'portrait-slot';
    if (c.portraits?.[gender]) {
      const img = document.createElement('img');
      img.src = `../characters/${c._stem}.${gender}.png`;
      slot.appendChild(img);
    } else {
      slot.textContent = `no ${gender} portrait`;
    }
    const lbl = document.createElement('div');
    lbl.className = 'label';
    lbl.textContent = gender;
    slot.appendChild(lbl);
    hero.appendChild(slot);
  }
  body.appendChild(hero);

  body.appendChild(sectionDl('Race — ' + c.race, [
    ['pitch', race?.visual?.pitch],
    ['archetype', race?.archetype],
    ['cultural traits', race?.cultural_traits],
  ]));
  body.appendChild(sectionDl(`Class — ${cls?.internal_label || 'class ' + c.class_id}`, [
    ['pitch', cls?.visual?.pitch],
    ['armor', cls?.visual?.armor],
    ['weapon', cls?.visual?.weapon],
    ['pose', cls?.visual?.pose],
    ['primary roles', (cls?.primary_roles || []).join(', ')],
    ['position', cls ? `M${cls.position.might} / A${cls.position.arcana} / F${cls.position.finesse}` : null],
  ]));
  body.appendChild(sectionDl(`Faction — ${c.faction}`, [
    ['pitch', faction?.visual?.pitch],
    ['palette', faction?.visual?.palette],
    ['heraldry', faction?.visual?.heraldry],
    ['garment style', faction?.visual?.garment_style],
  ]));
}

function sectionDl(title, fields) {
  const sec = document.createElement('div');
  sec.className = 'page-section';
  const h = document.createElement('h3');
  h.textContent = title;
  sec.appendChild(h);
  const dl = document.createElement('dl');
  for (const [k, v] of fields) {
    if (v == null || v === '') continue;
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    dd.textContent = String(v);
    dl.appendChild(dt);
    dl.appendChild(dd);
  }
  sec.appendChild(dl);
  return sec;
}

function showRaceDetail(r) {
  const body = $('#detail-body');
  body.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = r.id.replace(/_/g, ' ');
  body.appendChild(h);
  const m = document.createElement('div');
  m.className = 'meta';
  m.textContent = `${r.archetype} · ${r.faction}`;
  body.appendChild(m);

  // portraits row
  const row = document.createElement('div');
  row.style.cssText = 'display:flex;gap:12px;margin-bottom:16px;flex-wrap:wrap';
  for (const gender of ['male', 'female']) {
    if (!r.portraits?.[gender]) continue;
    const wrap = document.createElement('div');
    wrap.style.cssText = 'flex:1;min-width:200px;max-width:280px';
    const img = document.createElement('img');
    img.src = racePortraitUrl(r, gender);
    img.style.cssText = 'width:100%;border-radius:6px;display:block';
    wrap.appendChild(img);
    const cap = document.createElement('div');
    cap.style.cssText = 'text-align:center;font-size:11px;color:var(--text-dim);margin-top:4px';
    cap.textContent = gender;
    wrap.appendChild(cap);
    row.appendChild(wrap);
  }
  body.appendChild(row);

  const dl = document.createElement('dl');
  const fields = [
    ['favored class', r.favored_class],
    ['cultural traits', r.cultural_traits],
    ['pitch', r.visual?.pitch],
    ['signature', r.visual?.signature],
    ['body', r.visual?.body],
    ['features', r.visual?.features],
    ['hair', r.visual?.hair_style],
    ['silhouette', r.visual?.silhouette],
    ['attire', r.visual?.attire_baseline],
    ['distinguishing marks', r.visual?.distinguishing_marks],
    ['tags', r.visual?.tags],
    ['negative tags', r.visual?.negative_tags],
    ['file', r._file],
  ];
  for (const [k, v] of fields) {
    if (v == null || v === '') continue;
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    dd.textContent = String(v);
    dl.appendChild(dt);
    dl.appendChild(dd);
  }
  body.appendChild(dl);
  $('#detail-overlay').classList.remove('hidden');
}

// --- orders ---

function renderOrders() {
  const g = $('#order-grid');
  if (!g) return;
  g.innerHTML = '';
  for (const o of state.data.orders || []) {
    const card = document.createElement('div');
    const cls = state.data.classes.find((c) => c.class_id === o.archetype_id);
    const dominant = cls?.dominant_pillar?.[0] || 'arcana';
    card.className = `card combo-card ${o.faction} pillar-${dominant}`;

    const pair = document.createElement('div');
    pair.className = 'portrait-pair';
    for (const gender of ['male', 'female']) {
      const slot = document.createElement('div');
      slot.className = 'p';
      if (o.portraits?.[gender]) {
        const img = document.createElement('img');
        img.src = `../characters/${o.id}.${gender}.png`;
        img.loading = 'lazy';
        slot.appendChild(img);
      } else {
        slot.textContent = gender[0];
      }
      pair.appendChild(slot);
    }
    card.appendChild(pair);

    const title = document.createElement('div');
    title.className = 'card-title';
    title.textContent = o.player_facing?.class_name || o.id;
    card.appendChild(title);

    const desc = document.createElement('div');
    desc.className = 'card-desc';
    desc.textContent = o.aesthetic?.pitch || '';
    card.appendChild(desc);

    const meta = document.createElement('div');
    meta.className = 'card-meta';
    meta.innerHTML =
      `<span class="chip">${o.faction === 'faction_a' ? 'Concord' : 'Rend'}</span>` +
      `<span class="chip">${cls?.internal_label || 'archetype ' + o.archetype_id}</span>`;
    card.appendChild(meta);

    card.addEventListener('click', () => { window.location.hash = `order/${o.id}`; });
    g.appendChild(card);
  }
}

function renderOrderPage(orderId) {
  const o = (state.data.orders || []).find((x) => x.id === orderId);
  const body = $('#page-body');
  body.innerHTML = '';
  if (!o) {
    body.textContent = `order ${orderId} not found`;
    return;
  }
  $('#page-back').href = '#orders';

  const cls = state.data.classes.find((c) => c.class_id === o.archetype_id);

  const title = document.createElement('h1');
  title.className = 'page-title';
  title.textContent = o.player_facing?.class_name || o.id;
  body.appendChild(title);

  const sub = document.createElement('div');
  sub.className = 'page-subtitle';
  sub.textContent = `${o.faction === 'faction_a' ? 'Concord' : 'Rend'} · ${cls?.internal_label || 'archetype'} (M${o.archetype_position.might}/A${o.archetype_position.arcana}/F${o.archetype_position.finesse})`;
  body.appendChild(sub);

  // portrait hero row
  const hero = document.createElement('div');
  hero.className = 'page-hero';
  for (const gender of ['male', 'female']) {
    const slot = document.createElement('div');
    slot.className = 'portrait-slot';
    if (o.portraits?.[gender]) {
      const img = document.createElement('img');
      img.src = `../characters/${o.id}.${gender}.png`;
      slot.appendChild(img);
    } else {
      slot.textContent = `no ${gender} portrait`;
    }
    const lbl = document.createElement('div');
    lbl.className = 'label';
    lbl.textContent = gender;
    slot.appendChild(lbl);
    hero.appendChild(slot);
  }
  body.appendChild(hero);

  body.appendChild(sectionDl('Player-Facing', [
    ['class name', o.player_facing?.class_name],
    ['title singular', o.player_facing?.title_singular],
    ['title plural', o.player_facing?.title_plural],
  ]));
  body.appendChild(sectionDl('Aesthetic', [
    ['pitch', o.aesthetic?.pitch],
    ['palette shift', o.aesthetic?.palette_shift],
    ['motif', o.aesthetic?.motif],
  ]));
  const s = o.schools_taught || {};
  body.appendChild(sectionDl('Schools Taught', [
    ['arcana', (s.arcana || []).join(', ')],
    ['might', (s.might || []).join(', ')],
    ['finesse', (s.finesse || []).join(', ')],
  ]));
  // Specs
  if ((o.specs || []).length) {
    const sec = document.createElement('div');
    sec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Specs';
    sec.appendChild(h);
    const note = document.createElement('div');
    note.style.cssText = 'color:var(--text-dim);font-size:12px;margin-bottom:10px';
    note.textContent = 'Role emphases within this Class — same schools, different priorities';
    sec.appendChild(note);
    const grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill, minmax(240px, 1fr));gap:8px';
    for (const sp of o.specs) {
      const card = document.createElement('div');
      card.style.cssText =
        'background:var(--surface);border:1px solid var(--border);' +
        'border-radius:4px;padding:10px;display:flex;flex-direction:column;gap:4px';
      card.innerHTML =
        `<div style="font-weight:600;font-size:13px">${sp.name}</div>` +
        `<div><span class="chip">${sp.emphasis}</span></div>` +
        (sp.description ? `<div style="color:var(--text-dim);font-size:11px;line-height:1.4">${sp.description}</div>` : '') +
        ((sp.schools_focus || []).length
          ? `<div style="font-size:11px;color:var(--text-faint);margin-top:2px">focus: ${sp.schools_focus.join(', ')}</div>`
          : '');
      grid.appendChild(card);
    }
    sec.appendChild(grid);
    body.appendChild(sec);
  }

  body.appendChild(sectionDl('Lore', [
    ['founded', o.lore?.founded],
    ['home', o.lore?.home],
    ['oath', o.lore?.oath],
    ['patron', o.lore?.patron],
    ['recruitment', o.lore?.recruitment],
    ['doctrine', o.lore?.doctrine],
  ]));

  // Institutional home: primary / secondary / tertiary as chips
  const insts = o.institutions || {};
  if (insts.primary) {
    const isec = document.createElement('div');
    isec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Institutions';
    isec.appendChild(h);
    const note = document.createElement('div');
    note.style.cssText = 'color:var(--text-dim);font-size:12px;margin-bottom:10px';
    note.textContent = 'Home institution (primary) + training affiliations';
    isec.appendChild(note);
    const row = document.createElement('div');
    row.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px';
    const slots = [
      ['primary', insts.primary],
      ['secondary', insts.secondary],
      ['tertiary', insts.tertiary],
    ];
    for (const [slot, iid] of slots) {
      if (!iid) continue;
      const inst = state.data.institutions?.find((i) => i.id === iid);
      if (!inst) continue;
      const a = document.createElement('a');
      a.href = `#institution/${iid}`;
      a.style.cssText =
        'text-decoration:none;background:var(--surface);border:1px solid var(--border);' +
        'padding:6px 10px;border-radius:4px;color:var(--text);font-size:12px;' +
        'display:flex;flex-direction:column;gap:2px';
      a.innerHTML =
        `<span style="color:var(--text-faint);font-size:10px;text-transform:uppercase;letter-spacing:0.08em">${slot}</span>` +
        `<span style="font-weight:600">${inst.name}</span>` +
        `<span style="color:var(--text-dim);font-size:11px">${inst.tradition || ''}</span>`;
      row.appendChild(a);
    }
    isec.appendChild(row);
    if (o.chapter) {
      const c = document.createElement('div');
      c.style.cssText = 'color:var(--text-faint);font-size:11px;margin-top:8px';
      c.textContent = `chapter: ${o.chapter}`;
      isec.appendChild(c);
    }
    body.appendChild(isec);
  }

  // Archetype link
  if (cls) {
    const linkSec = document.createElement('div');
    linkSec.className = 'page-section';
    const h = document.createElement('h3');
    h.textContent = 'Underlying Archetype';
    linkSec.appendChild(h);
    linkSec.appendChild(archetypeChip(cls));
    body.appendChild(linkSec);
  }

  body.appendChild(sectionDl('Metadata', [
    ['id', o.id],
    ['file', o._file],
  ]));
}

// --- institutions ---

function renderInstitutions() {
  const g = $('#institution-grid');
  if (!g) return;
  g.innerHTML = '';
  for (const inst of state.data.institutions || []) {
    const card = document.createElement('div');
    card.className = `card ${inst.faction}`;
    if (inst.has_emblem) {
      const emb = document.createElement('div');
      emb.className = 'emblem';
      const img = document.createElement('img');
      img.src = `../emblems/institution_${inst.id}.png`;
      img.loading = 'lazy';
      emb.appendChild(img);
      card.appendChild(emb);
    }
    const body = document.createElement('div');
    const majors = flattenCurriculum(inst.curriculum?.major);
    const majorsText = majors.length ? majors.join(', ') : '—';
    body.innerHTML =
      `<div class="card-title">${inst.name}</div>` +
      `<div class="card-desc" style="color:var(--text-dim);font-style:italic">${inst.tradition || ''}</div>` +
      `<div style="font-size:11px;color:var(--text-faint);margin-top:4px">major: ${majorsText}</div>` +
      `<div class="card-meta" style="margin-top:6px">` +
      `<span class="chip">${inst.faction === 'faction_a' ? 'Concord' : 'Rend'}</span>` +
      `<span class="chip">${(inst.chapters || []).length} chapters</span>` +
      `</div>`;
    card.appendChild(body);
    card.addEventListener('click', () => { window.location.hash = `institution/${inst.id}`; });
    g.appendChild(card);
  }
}

function flattenCurriculum(pillarDict) {
  if (!pillarDict) return [];
  const out = [];
  for (const pillar of ['arcana', 'might', 'finesse']) {
    for (const s of (pillarDict[pillar] || [])) out.push(s);
  }
  return out;
}

function renderInstitutionPage(instId) {
  const inst = (state.data.institutions || []).find((i) => i.id === instId);
  const body = $('#page-body');
  body.innerHTML = '';
  if (!inst) {
    body.textContent = `institution ${instId} not found`;
    return;
  }
  $('#page-back').href = '#institutions';

  const title = document.createElement('h1');
  title.className = 'page-title';
  title.textContent = inst.name;
  body.appendChild(title);

  const sub = document.createElement('div');
  sub.className = 'page-subtitle';
  sub.textContent = `${inst.tradition || 'Tradition'} · ${inst.faction === 'faction_a' ? 'Concord' : 'Rend'}`;
  body.appendChild(sub);

  // Curriculum section
  const curSec = document.createElement('div');
  curSec.className = 'page-section';
  const ch = document.createElement('h3');
  ch.textContent = 'Curriculum';
  curSec.appendChild(ch);
  const cur = inst.curriculum || {};
  const curTable = document.createElement('dl');
  const curRows = [
    ['major schools', describePillars(cur.major)],
    ['secondary schools', describePillars(cur.secondary)],
  ];
  for (const [k, v] of curRows) {
    if (!v) continue;
    const dt = document.createElement('dt');
    dt.textContent = k;
    const dd = document.createElement('dd');
    dd.innerHTML = v;
    curTable.appendChild(dt);
    curTable.appendChild(dd);
  }
  curSec.appendChild(curTable);
  body.appendChild(curSec);

  // Chapters
  const chapterIds = inst.chapters || [];
  if (chapterIds.length) {
    const chSec = document.createElement('div');
    chSec.className = 'page-section';
    const hh = document.createElement('h3');
    hh.textContent = `Chapters (${chapterIds.length})`;
    chSec.appendChild(hh);
    const note = document.createElement('div');
    note.style.cssText = 'color:var(--text-dim);font-size:12px;margin-bottom:10px';
    note.textContent = 'Player-facing classes that call this institution home';
    chSec.appendChild(note);
    const grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill, minmax(240px, 1fr));gap:10px';
    // Join chapters with the orders they produce
    const orders = state.data.orders || [];
    for (const oId of (inst.orders_produced || [])) {
      const o = orders.find((x) => x.id === oId);
      if (!o) continue;
      const cls = state.data.classes.find((c) => c.class_id === o.archetype_id);
      const card = document.createElement('a');
      card.href = `#order/${o.id}`;
      card.style.cssText =
        'text-decoration:none;color:var(--text);background:var(--surface);' +
        'border:1px solid var(--border);border-radius:6px;padding:10px;' +
        'display:flex;flex-direction:column;gap:4px;transition:border-color 0.1s';
      card.addEventListener('mouseenter', () => (card.style.borderColor = 'var(--arcana)'));
      card.addEventListener('mouseleave', () => (card.style.borderColor = 'var(--border)'));
      card.innerHTML =
        `<div style="font-weight:600;font-size:13px">${o.player_facing?.class_name || o.id}</div>` +
        `<div style="color:var(--text-dim);font-size:11px">${cls?.abstract_name || cls?.internal_label || 'archetype'}` +
        (cls ? ` · M${cls.position.might}/A${cls.position.arcana}/F${cls.position.finesse}` : '') +
        `</div>` +
        (o.aesthetic?.pitch ? `<div style="color:var(--text-faint);font-size:11px;line-height:1.4;margin-top:4px">${escape(o.aesthetic.pitch.slice(0, 120))}${o.aesthetic.pitch.length > 120 ? '…' : ''}</div>` : '');
      grid.appendChild(card);
    }
    chSec.appendChild(grid);
    body.appendChild(chSec);
  }

  body.appendChild(sectionDl('Lore', [
    ['description', inst.lore?.description],
    ['founded', inst.lore?.founded],
    ['home', inst.lore?.home],
    ['doctrine', inst.lore?.doctrine],
    ['patron', inst.lore?.patron],
    ['recruitment', inst.lore?.recruitment],
  ]));

  body.appendChild(sectionDl('Aesthetic', [
    ['pitch', inst.aesthetic?.pitch],
    ['palette', inst.aesthetic?.palette],
    ['motif', inst.aesthetic?.motif],
  ]));

  body.appendChild(sectionDl('Metadata', [
    ['id', inst.id],
    ['file', inst._file],
  ]));
}

function describePillars(pillarDict) {
  if (!pillarDict) return null;
  const parts = [];
  for (const pillar of ['arcana', 'might', 'finesse']) {
    const schools = pillarDict[pillar] || [];
    if (!schools.length) continue;
    const color = `var(--${pillar})`;
    parts.push(
      `<span style="color:${color};text-transform:uppercase;font-size:11px;letter-spacing:0.05em">${pillar}:</span> ` +
      schools.join(', '),
    );
  }
  return parts.join('<br>');
}

// --- factions ---

function renderFactions() {
  const g = $('#faction-grid');
  g.innerHTML = '';
  for (const f of state.data.factions) {
    const c = document.createElement('div');
    c.className = `card ${f.id}`;
    if (f.has_emblem) {
      const emb = document.createElement('div');
      emb.className = 'emblem';
      const img = document.createElement('img');
      img.src = `../emblems/${f.id}.png`;
      img.loading = 'lazy';
      emb.appendChild(img);
      c.appendChild(emb);
    }
    const body = document.createElement('div');
    body.innerHTML =
      `<div class="card-title">${f.id}</div>` +
      `<div class="card-desc">morality: <span class="morality-${f.morality_alignment}">${f.morality_alignment}</span></div>` +
      `<div class="card-meta">` +
      (f.allowed_school_moralities || []).map((m) => `<span class="chip morality-${m}">${m}</span>`).join('') +
      `</div>`;
    c.appendChild(body);
    c.addEventListener('click', () =>
      showDetail(
        f.id,
        `morality ${f.morality_alignment}`,
        [
          ['pitch', f.visual?.pitch],
          ['palette', f.visual?.palette],
          ['heraldry', f.visual?.heraldry],
          ['architecture', f.visual?.architecture],
          ['garment style', f.visual?.garment_style],
          ['damage patina', f.visual?.damage_patina],
          ['material accents', f.visual?.material_accents],
          ['allowed school moralities', (f.allowed_school_moralities || []).join(', ')],
          ['forbidden school moralities', (f.forbidden_school_moralities || []).join(', ')],
          ['allowed arcana schools', (f.allowed_schools?.arcana || []).join(', ')],
          ['allowed might schools', (f.allowed_schools?.might || []).join(', ')],
          ['allowed finesse schools', (f.allowed_schools?.finesse || []).join(', ')],
          ['forbidden schools', JSON.stringify(f.forbidden_schools)],
          ['file', f._file],
        ],
        f.has_emblem ? `../emblems/${f.id}.png` : null,
      ),
    );
    g.appendChild(c);
  }
}

load();
