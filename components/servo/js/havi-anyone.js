// @ts-check
/// <reference path="havi.d.ts" />

/**
 * @typedef {{ coord: string, ops: string }} AclRule
 */

/** @type {AclRule[]} */
let rules = [];
/** @type {string[]} */
let newPerms = ['r', '.', '.'];
let isDirty = false;

/** @param {string} text @param {boolean} isError */
function showMessage(text, isError) {
    const el = document.getElementById('message');
    if (!el) return;
    el.className = 'message ' + (isError ? 'error' : 'success');
    el.textContent = text;
    el.style.display = 'block';
    setTimeout(() => el.style.display = 'none', 5000);
}

function markDirty() {
    isDirty = true;
    const el = document.getElementById('dirtyIndicator');
    if (el) el.style.display = 'inline';
}

function markClean() {
    isDirty = false;
    const el = document.getElementById('dirtyIndicator');
    if (el) el.style.display = 'none';
}

/** @param {string} p */
function permClass(p) {
    if (p === 'r' || p === 'w' || p === 'l') return 'perm-grant';
    if (p === 'd') return 'perm-deny';
    return 'perm-inherit';
}

/** @param {string} current @param {number} pos */
function nextPerm(current, pos) {
    const grants = ['r', 'w', 'l'][pos];
    if (current === grants) return 'd';
    if (current === 'd') return '.';
    return grants;
}

/** @param {string} a @param {string} b */
function sortCoords(a, b) {
    const norm = /** @param {string} s */ (s) => s.replace(/\|/g, '\x01').replace(/\//g, '\x02');
    return norm(a).localeCompare(norm(b));
}

/**
 * @param {string} coord
 * @param {Set<string>} coordSet
 * @returns {string|null}
 */
function findLogicalParent(coord, coordSet) {
    /** @type {string[]} */
    const prefixes = [];
    let current = coord;

    if (current.endsWith('/') && current.length > 3) {
        prefixes.push(current.slice(0, -1));
        current = current.slice(0, -1);
    }

    while (current.length > 2) {
        const lastSlash = current.lastIndexOf('/');
        if (lastSlash <= 1) break;
        const parent = current.slice(0, lastSlash);
        prefixes.push(parent + '/');
        prefixes.push(parent);
        current = parent;
    }

    for (const prefix of prefixes) {
        if (prefix !== coord && coordSet.has(prefix)) {
            return prefix;
        }
    }
    return null;
}

function buildTree() {
    const sorted = [...rules].sort((a, b) => sortCoords(a.coord, b.coord));
    const coordSet = new Set(sorted.map(r => r.coord));

    /** @type {AclRule[]} */
    const roots = [];
    /** @type {Map<string, AclRule[]>} */
    const childrenOf = new Map();

    for (const rule of sorted) {
        const parent = findLogicalParent(rule.coord, coordSet);
        if (parent === null) {
            roots.push(rule);
        } else {
            if (!childrenOf.has(parent)) {
                childrenOf.set(parent, []);
            }
            /** @type {AclRule[]} */ (childrenOf.get(parent)).push(rule);
        }
    }

    return { roots, childrenOf };
}

/**
 * @param {AclRule} rule
 * @param {Map<string, AclRule[]>} childrenOf
 * @param {number} depth
 * @returns {string}
 */
function renderRuleNode(rule, childrenOf, depth) {
    const children = childrenOf.get(rule.coord) || [];
    const hasChildren = children.length > 0;
    const idx = rules.findIndex(r => r.coord === rule.coord);

    const rowHtml = `
        <div class="acl-row" data-depth="${depth}">
            <span class="acl-coord">${rule.coord}</span>
            <button class="perm-btn ${permClass(rule.ops[0])}" onclick="togglePerm(${idx}, 0)">${rule.ops[0]}</button>
            <button class="perm-btn ${permClass(rule.ops[1])}" onclick="togglePerm(${idx}, 1)">${rule.ops[1]}</button>
            <button class="perm-btn ${permClass(rule.ops[2])}" onclick="togglePerm(${idx}, 2)">${rule.ops[2]}</button>
            <button class="remove-btn" onclick="removeRule(${idx})" title="Remove rule">\u00d7</button>
        </div>
    `;

    if (!hasChildren) {
        return `<div class="leaf-rule">${rowHtml}</div>`;
    }

    return `
        <details open>
            <summary>${rowHtml}</summary>
            ${children.map(child => renderRuleNode(child, childrenOf, depth + 1)).join('')}
        </details>
    `;
}

function renderRules() {
    const list = document.getElementById('rulesList');
    if (!list) return;
    if (rules.length === 0) {
        list.innerHTML = '<div class="acl-row"><span class="empty" style="grid-column: 1/-1;">No rules configured</span></div>';
        return;
    }

    const { roots, childrenOf } = buildTree();
    list.innerHTML = '<div class="acl-tree">' +
        roots.map(root => renderRuleNode(root, childrenOf, 0)).join('') +
        '</div>';
}

/** @param {number} idx @param {number} pos */
function togglePerm(idx, pos) {
    const ops = rules[idx].ops.split('');
    ops[pos] = nextPerm(ops[pos], pos);
    rules[idx].ops = ops.join('');
    markDirty();
    renderRules();
}

/** @param {number} pos */
function toggleNewPerm(pos) {
    newPerms[pos] = nextPerm(newPerms[pos], pos);
    const ids = ['newR', 'newW', 'newL'];
    const btn = document.getElementById(ids[pos]);
    if (btn) {
        btn.textContent = newPerms[pos];
        btn.className = 'perm-btn ' + permClass(newPerms[pos]);
    }
}

/** @param {number} idx */
function removeRule(idx) {
    rules.splice(idx, 1);
    markDirty();
    renderRules();
}

function addRule() {
    const input = /** @type {HTMLInputElement|null} */ (document.getElementById('newCoord'));
    if (!input) return;
    const coord = input.value.trim();
    if (!coord) {
        showMessage('Please enter a coordinate', true);
        return;
    }
    if (!coord.startsWith('//')) {
        showMessage('Coordinate must start with //', true);
        return;
    }
    if (rules.some(r => r.coord === coord)) {
        showMessage('Rule for this coordinate already exists', true);
        return;
    }
    rules.push({ coord, ops: newPerms.join('') });
    rules.sort((a, b) => sortCoords(a.coord, b.coord));
    input.value = '';
    newPerms = ['r', '.', '.'];
    const newR = document.getElementById('newR');
    if (newR) { newR.textContent = 'r'; newR.className = 'perm-btn perm-grant'; }
    const newW = document.getElementById('newW');
    if (newW) { newW.textContent = '.'; newW.className = 'perm-btn perm-inherit'; }
    const newL = document.getElementById('newL');
    if (newL) { newL.textContent = '.'; newL.className = 'perm-btn perm-inherit'; }
    markDirty();
    renderRules();
}

async function loadRules() {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const packet = await window.ring0.get('//repo/admin/ring1/anyone/setup/|');
        rules = [];

        for (const h of packet.getHeaders('ACL-Rule')) {
            const match = h.match(/^([rwld.])([rwld.])([rwld.])\s+(.+)$/);
            if (match) {
                rules.push({
                    ops: match[1] + match[2] + match[3],
                    coord: match[4]
                });
            }
        }

        markClean();
        renderRules();
    } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg && msg.includes('NOT_FOUND')) {
            rules = [];
            markClean();
            renderRules();
            showMessage('No anyone account configured. Add rules and save to create it.', false);
        } else {
            showMessage('Failed to load rules: ' + msg, true);
        }
    }
}

async function saveRules() {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        rules.sort((a, b) => sortCoords(a.coord, b.coord));

        const headers = [
            'Group: repo',
            'App: admin',
            'Location: ring1/anyone/setup',
            'Ring1-Name: anyone',
            ...rules.map(r => 'ACL-Rule: ' + r.ops + ' ' + r.coord)
        ];

        await window.ring0.add({ headers: headers, data: '' });
        showMessage('Rules saved successfully', false);
        markClean();
        renderRules();
    } catch (e) {
        showMessage('Failed to save rules: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

window.addEventListener('beforeunload', (e) => {
    if (isDirty) {
        e.preventDefault();
    }
});

loadRules();
