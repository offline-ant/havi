// @ts-check
/// <reference path="havi.d.ts" />

/**
 * @typedef {{ coord: string, ops: string }} AclRule
 * @typedef {{ name: string, rules: AclRule[], expire: string|null, hash: string|null, isExpired: boolean }} AccountInfo
 */

/** @type {string|null} */
let editingAccount = null;
/** @type {AclRule[]} */
let editRules = [];
/** @type {string[]} */
let editNewPerms = ['r', '.', '.'];

/** @param {string} text @param {boolean} isError */
function showMessage(text, isError) {
    const el = document.getElementById('message');
    if (!el) return;
    el.className = 'message ' + (isError ? 'error' : 'success');
    el.textContent = text;
    el.style.display = 'block';
    setTimeout(() => el.style.display = 'none', 5000);
}

/** @param {string} name */
function categorize(name) {
    if (['ring0', 'anyone', 'guest'].includes(name)) return 'system';
    if (name.startsWith('site:') || name.startsWith('HAVI-site:')) return 'sandbox';
    return 'custom';
}

/** @param {string} name */
function parseSandboxName(name) {
    const match = name.match(/^(?:site:|HAVI-site:)(.+)#(.*)$/);
    if (match) return { group: match[1], app: match[2] || '(all)' };
    return null;
}

/** @param {string|null} tai */
function formatTai(tai) {
    if (!tai) return null;
    const seconds = parseInt(tai.split(':')[0], 10);
    return new Date(seconds * 1000).toLocaleDateString();
}

/** @param {string|null} tai */
function isExpired(tai) {
    if (!tai) return false;
    const seconds = parseInt(tai.split(':')[0], 10);
    return Date.now() / 1000 > seconds;
}

/**
 * @param {string} ruleStr
 * @returns {AclRule|null}
 */
function parseRule(ruleStr) {
    const match = ruleStr.match(/^([rwld.])([rwld.])([rwld.])\s+(.+)$/);
    if (match) {
        return { ops: match[1] + match[2] + match[3], coord: match[4] };
    }
    return null;
}

async function loadAccounts() {
    /** @type {{ system: AccountInfo[], sandbox: AccountInfo[], custom: AccountInfo[] }} */
    const accounts = { system: [], sandbox: [], custom: [] };

    try {
        if (!window.ring0) return accounts;
        const names = await window.ring0.list('//repo/admin/ring1/');

        for (const name of names) {
            const cleanName = name.replace(/\/$/, '');
            try {
                const packet = await window.ring0.get('//repo/admin/ring1/' + cleanName + '/setup/|');
                const ruleHeaders = packet.getHeaders('ACL-Rule');
                const expire = packet.getHeader('Ring1-Expire');
                const parsedRules = ruleHeaders.map(parseRule).filter(/** @param {AclRule|null} r */ r => r !== null);

                const category = categorize(cleanName);
                accounts[category].push({
                    name: cleanName,
                    rules: /** @type {AclRule[]} */ (parsedRules),
                    expire,
                    hash: packet.hash,
                    isExpired: isExpired(expire)
                });
            } catch (_e) {
                const category = categorize(cleanName);
                accounts[category].push({
                    name: cleanName,
                    rules: [],
                    expire: null,
                    hash: null,
                    isExpired: false
                });
            }
        }
    } catch (_e) {
        // No ring1 directory yet
    }

    return accounts;
}

async function loadRequests() {
    /** @type {{ name: string, rules: AclRule[], hash: string }[]} */
    const requests = [];

    try {
        if (!window.ring0) return requests;
        const names = await window.ring0.list('//repo/admin/request/ring1/');

        for (const name of names) {
            const cleanName = name.replace(/\/$/, '');
            try {
                const packet = await window.ring0.get('//repo/admin/request/ring1/' + cleanName + '/setup/|');
                const ruleHeaders = packet.getHeaders('ACL-Rule');
                const parsedRules = ruleHeaders.map(parseRule).filter(/** @param {AclRule|null} r */ r => r !== null);
                requests.push({ name: cleanName, rules: /** @type {AclRule[]} */ (parsedRules), hash: packet.hash });
            } catch (_e) {
                // Request directory exists but no setup
            }
        }
    } catch (_e) {
        // No requests directory
    }

    return requests;
}

/** @param {AccountInfo[]} accounts */
function renderSystemAccounts(accounts) {
    const container = document.getElementById('systemAccounts');
    if (!container) return;

    if (accounts.length === 0) {
        container.innerHTML = '<p class="empty">No system accounts</p>';
        return;
    }

    let html = '';
    for (const acc of accounts) {
        let meta = '';
        let buttons = '';

        if (acc.name === 'ring0') {
            meta = '(admin - full access)';
        } else if (acc.name === 'anyone' || acc.name === 'guest') {
            meta = acc.rules.length + ' rule' + (acc.rules.length !== 1 ? 's' : '');
            buttons = `<a href="havi:///anyone" class="btn-small secondary" style="text-decoration: none;">Edit on Anyone</a>`;
        }

        const rulesHtml = acc.name !== 'ring0' && acc.rules.length > 0
            ? '<div class="account-rules">' + acc.rules.map(r =>
                `<div class="account-rule">${r.ops} ${r.coord}</div>`
              ).join('') + '</div>'
            : '';

        html += `
            <div class="account-item">
                <div class="account-header">
                    <div>
                        <span class="account-name system">${acc.name}</span>
                        <span class="account-meta">${meta}</span>
                    </div>
                    <div class="btn-group">${buttons}</div>
                </div>
                ${rulesHtml}
            </div>
        `;
    }

    container.innerHTML = html;
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

/** @param {string} accountName @returns {string} */
function renderInlineEditor(accountName) {
    let rowsHtml = '';
    for (let i = 0; i < editRules.length; i++) {
        const rule = editRules[i];
        rowsHtml += `
            <div class="acl-row">
                <span class="acl-coord">${rule.coord}</span>
                <button class="perm-btn ${permClass(rule.ops[0])}" onclick="toggleEditPerm(${i}, 0)">${rule.ops[0]}</button>
                <button class="perm-btn ${permClass(rule.ops[1])}" onclick="toggleEditPerm(${i}, 1)">${rule.ops[1]}</button>
                <button class="perm-btn ${permClass(rule.ops[2])}" onclick="toggleEditPerm(${i}, 2)">${rule.ops[2]}</button>
                <button class="remove-btn" onclick="removeEditRule(${i})" title="Remove rule">\u00d7</button>
            </div>
        `;
    }

    if (editRules.length === 0) {
        rowsHtml = '<div class="acl-row"><span class="empty" style="grid-column: 1/-1;">No rules configured</span></div>';
    }

    return `
        <div class="inline-editor">
            <div class="acl-editor">
                <div class="acl-header">
                    <span>Coordinate</span>
                    <span title="Read">R</span>
                    <span title="Write">W</span>
                    <span title="List">L</span>
                    <span></span>
                </div>
                <div id="editRulesList">${rowsHtml}</div>
                <div class="add-row">
                    <input type="text" id="editNewCoord" placeholder="//group/app/path">
                    <button class="perm-btn ${permClass(editNewPerms[0])}" id="editNewR" onclick="toggleEditNewPerm(0)">${editNewPerms[0]}</button>
                    <button class="perm-btn ${permClass(editNewPerms[1])}" id="editNewW" onclick="toggleEditNewPerm(1)">${editNewPerms[1]}</button>
                    <button class="perm-btn ${permClass(editNewPerms[2])}" id="editNewL" onclick="toggleEditNewPerm(2)">${editNewPerms[2]}</button>
                    <button class="add-btn" onclick="addEditRule()" title="Add rule">+</button>
                </div>
            </div>
            <div class="editor-buttons">
                <button onclick="saveEdit('${accountName}')">Save</button>
                <button class="secondary" onclick="cancelEdit()">Cancel</button>
            </div>
        </div>
    `;
}

/** @param {AccountInfo[]} accounts */
function renderSandboxAccounts(accounts) {
    const container = document.getElementById('sandboxAccounts');
    if (!container) return;

    if (accounts.length === 0) {
        container.innerHTML = '<p class="empty">None</p>';
        return;
    }

    let html = '';
    for (const acc of accounts) {
        const parsed = parseSandboxName(acc.name);
        const displayName = parsed ? `//${parsed.group}/${parsed.app}` : acc.name;

        const rulesHtml = acc.rules.length > 0
            ? '<div class="account-rules">' + acc.rules.map(r =>
                `<div class="account-rule">${r.ops} ${r.coord}</div>`
              ).join('') + '</div>'
            : '<div class="account-rules"><div class="account-rule empty">No rules</div></div>';

        const editorHtml = editingAccount === acc.name ? renderInlineEditor(acc.name) : '';
        const safeId = acc.name.replace(/[^a-zA-Z0-9]/g, '_');

        html += `
            <div class="account-item" id="account-${safeId}">
                <div class="account-header">
                    <div>
                        <span class="account-name sandbox">${acc.name}</span>
                        <span class="sandbox-display">${displayName}</span>
                    </div>
                    <div class="btn-group">
                        <button class="btn-small secondary" onclick="startEdit('${acc.name}')">Edit</button>
                        <button class="btn-small danger" onclick="deleteAccount('${acc.name}', '${acc.hash}')">Delete</button>
                    </div>
                </div>
                ${editingAccount !== acc.name ? rulesHtml : ''}
                ${editorHtml}
            </div>
        `;
    }

    container.innerHTML = html;
}

/** @param {AccountInfo[]} accounts */
function renderCustomAccounts(accounts) {
    const container = document.getElementById('customAccounts');
    if (!container) return;

    if (accounts.length === 0) {
        container.innerHTML = '<p class="empty">None</p>';
        return;
    }

    let html = '';
    for (const acc of accounts) {
        const expireHtml = acc.expire
            ? `<div class="account-rule ${acc.isExpired ? 'account-expired' : ''}">Expires: ${formatTai(acc.expire)}${acc.isExpired ? ' (EXPIRED)' : ''}</div>`
            : '';

        const rulesHtml = '<div class="account-rules">' +
            (acc.rules.length > 0
                ? acc.rules.map(r => `<div class="account-rule">${r.ops} ${r.coord}</div>`).join('')
                : '<div class="account-rule empty">No rules</div>') +
            expireHtml +
            '</div>';

        const editorHtml = editingAccount === acc.name ? renderInlineEditor(acc.name) : '';
        const safeId = acc.name.replace(/[^a-zA-Z0-9]/g, '_');

        html += `
            <div class="account-item" id="account-${safeId}">
                <div class="account-header">
                    <div>
                        <span class="account-name">${acc.name}</span>
                    </div>
                    <div class="btn-group">
                        <button class="btn-small secondary" onclick="startEdit('${acc.name}')">Edit</button>
                        <button class="btn-small danger" onclick="deleteAccount('${acc.name}', '${acc.hash}')">Delete</button>
                    </div>
                </div>
                ${editingAccount !== acc.name ? rulesHtml : ''}
                ${editorHtml}
            </div>
        `;
    }

    container.innerHTML = html;
}

/** @param {{ name: string, rules: AclRule[], hash: string }[]} requests */
function renderRequests(requests) {
    const container = document.getElementById('pendingRequests');
    if (!container) return;

    if (requests.length === 0) {
        container.innerHTML = '<p class="empty">None</p>';
        return;
    }

    let html = '';
    for (const req of requests) {
        const rulesHtml = req.rules.length > 0
            ? '<div class="account-rules">' + req.rules.map(r =>
                `<div class="account-rule">Wants: ${r.ops} ${r.coord}</div>`
              ).join('') + '</div>'
            : '<div class="account-rules"><div class="account-rule empty">No rules specified</div></div>';

        html += `
            <div class="account-item request-item">
                <div class="account-header">
                    <div>
                        <span class="account-name">${req.name}</span>
                        <span class="account-meta">(pending)</span>
                    </div>
                    <div class="btn-group">
                        <button class="btn-small" onclick="approveRequest('${req.name}')">Approve</button>
                        <button class="btn-small danger" onclick="denyRequest('${req.hash}')">Deny</button>
                    </div>
                </div>
                ${rulesHtml}
            </div>
        `;
    }

    container.innerHTML = html;
}

/** @param {string} name */
async function startEdit(name) {
    editingAccount = name;
    editRules = [];
    editNewPerms = ['r', '.', '.'];

    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const packet = await window.ring0.get('//repo/admin/ring1/' + name + '/setup/|');
        const ruleHeaders = packet.getHeaders('ACL-Rule');
        editRules = /** @type {AclRule[]} */ (ruleHeaders.map(parseRule).filter(/** @param {AclRule|null} r */ r => r !== null));
    } catch (_e) {
        // No setup packet, start with empty rules
    }

    loadAll();
}

function cancelEdit() {
    editingAccount = null;
    editRules = [];
    loadAll();
}

/** @param {number} idx @param {number} pos */
function toggleEditPerm(idx, pos) {
    const ops = editRules[idx].ops.split('');
    ops[pos] = nextPerm(ops[pos], pos);
    editRules[idx].ops = ops.join('');
    loadAll();
}

/** @param {number} pos */
function toggleEditNewPerm(pos) {
    editNewPerms[pos] = nextPerm(editNewPerms[pos], pos);
    loadAll();
}

/** @param {number} idx */
function removeEditRule(idx) {
    editRules.splice(idx, 1);
    loadAll();
}

function addEditRule() {
    const input = /** @type {HTMLInputElement|null} */ (document.getElementById('editNewCoord'));
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
    if (editRules.some(r => r.coord === coord)) {
        showMessage('Rule for this coordinate already exists', true);
        return;
    }

    editRules.push({ coord, ops: editNewPerms.join('') });
    editRules.sort((a, b) => sortCoords(a.coord, b.coord));
    editNewPerms = ['r', '.', '.'];
    loadAll();
}

/** @param {string} name */
async function saveEdit(name) {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        editRules.sort((a, b) => sortCoords(a.coord, b.coord));

        const headers = [
            'Group: repo',
            'App: admin',
            'Location: ring1/' + name + '/setup',
            'Ring1-Name: ' + name,
            ...editRules.map(r => 'ACL-Rule: ' + r.ops + ' ' + r.coord)
        ];

        await window.ring0.add({ headers: headers, data: '' });
        showMessage('Rules saved for ' + name, false);
        editingAccount = null;
        editRules = [];
        loadAll();
    } catch (e) {
        showMessage('Failed to save rules: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

/**
 * @param {string} name
 * @param {string|null} hash
 */
async function deleteAccount(name, hash) {
    if (!confirm('Delete account "' + name + '"?')) return;

    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        if (hash) {
            await window.ring0.detach(hash);
        }
        showMessage('Deleted account: ' + name, false);
        loadAll();
    } catch (e) {
        showMessage('Failed to delete account: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

/** @param {string} name */
async function approveRequest(name) {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const reqPacket = await window.ring0.get('//repo/admin/request/ring1/' + name + '/setup/|');

        const ruleHeaders = reqPacket.getHeaders('ACL-Rule');
        const headers = [
            'Group: repo',
            'App: admin',
            'Location: ring1/' + name + '/setup',
            'Ring1-Name: ' + name,
            ...ruleHeaders.map(/** @param {string} r */ r => 'ACL-Rule: ' + r)
        ];

        await window.ring0.add({ headers: headers, data: reqPacket.text() });
        await window.ring0.detach(reqPacket.hash);
        showMessage('Approved account: ' + name, false);
        loadAll();
    } catch (e) {
        showMessage('Failed to approve request: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

/** @param {string} hash */
async function denyRequest(hash) {
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        await window.ring0.detach(hash);
        showMessage('Request denied', false);
        loadAll();
    } catch (e) {
        showMessage('Failed to deny request: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

async function loadAll() {
    const accounts = await loadAccounts();
    const requests = await loadRequests();

    renderSystemAccounts(accounts.system);
    renderSandboxAccounts(accounts.sandbox);
    renderCustomAccounts(accounts.custom);
    renderRequests(requests);
}

loadAll();
