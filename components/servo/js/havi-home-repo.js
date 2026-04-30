// @ts-check
/// <reference path="havi.d.ts" />

/** @param {string} text */
function escapeHtml(text) {
    return text
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
}

/** @param {string} name */
function grantInputId(name) {
    return 'grant-origin-' + encodeURIComponent(name);
}

/** @param {number} tsUnix */
function formatUnix(tsUnix) {
    if (!Number.isFinite(tsUnix) || tsUnix <= 0) return '-';
    return new Date(tsUnix * 1000).toISOString();
}

/** @param {string} text @param {boolean=} ok */
function setNamedClientMessage(text, ok) {
    const el = document.getElementById('namedClientMessage');
    if (!el) return;
    el.textContent = text;
    el.className = ok === undefined ? 'muted' : (ok ? 'success' : 'error');
}

/** @param {string} cmd @param {Record<string, string>=} params */
async function homeRepoRequest(cmd, params) {
    const query = new URLSearchParams({ cmd });
    if (params) {
        for (const [k, v] of Object.entries(params)) {
            query.set(k, String(v));
        }
    }
    const resp = await fetch('havi:///home-repo/api?' + query.toString());
    const json = await resp.json();
    if (!json.ok) throw new Error(String(json.error || 'home-repo api error'));
    return json.data;
}

/** @param {string} id @param {string} value */
function setText(id, value) {
    const el = document.getElementById(id);
    if (el) el.textContent = value;
}

async function loadStatus() {
    try {
        const runtime = await homeRepoRequest('runtime_status');
        setText('port', runtime.port == null ? '-' : String(runtime.port));
        setText('wsPort', runtime.wsPort == null ? '-' : String(runtime.wsPort));
        setText('quibPort', runtime.quibPort == null ? '-' : String(runtime.quibPort));
        setText('udpPort', runtime.udpPort == null ? '-' : String(runtime.udpPort));
        setText('status', runtime.status || runtime.mode || '-');
        setText('repoPath', runtime.mode === 'local'
            ? (runtime.packetStorePath || '(not available)')
            : (runtime.repoTarget || runtime.repoPath || '(remote home repo)'));
        setText('repoKey', runtime.verifyingKey || '(none)');
        setText('daemonStatus', runtime.status || '-');
        setText('daemonUptime', runtime.uptime || '-');
        setText('daemonBackend', runtime.backend || '-');
        setText('daemonVersion', runtime.version || '-');

        const card = document.getElementById('daemonInfoCard');
        if (card) card.style.display = '';

        const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
        if (nameEl) nameEl.value = runtime.repoName || 'havi-local';

        await loadNamedClients();
    } catch (e) {
        console.error('Failed to load home-repo status:', e);
        setText('port', 'Error');
        setText('status', 'Error');
        setText('repoKey', '(error)');
    }
}

async function saveRepoName() {
    const nameInput = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
    const msgEl = document.getElementById('nameMessage');
    if (!nameInput || !msgEl) return;

    const newName = nameInput.value.trim();
    if (!newName) {
        msgEl.textContent = 'Please enter a repo name';
        return;
    }

    try {
        const data = await homeRepoRequest('set_repo_name', { name: newName });
        msgEl.textContent = 'Repo name updated: ' + (data.name || newName);
    } catch (e) {
        msgEl.textContent = 'Failed to save: ' + (e instanceof Error ? e.message : String(e));
    }
}

/** @typedef {{ name: string, endpoint: string, signer: string }} NamedClientEntry */
/** @typedef {{ origin: string, client_name: string, granted_at_unix: number }} NamedClientGrantEntry */
/** @typedef {{ id: number, origin: string, client_name: string, revoked_at_unix: number }} NamedClientRevocationEntry */

/** @param {{ clients: NamedClientEntry[], grants: NamedClientGrantEntry[], revocations: NamedClientRevocationEntry[] }} data */
function renderNamedClients(data) {
    const listEl = document.getElementById('namedClientsList');
    const revocationsEl = document.getElementById('namedClientRevocations');
    if (!listEl || !revocationsEl) return;

    if (!data.clients.length) {
        listEl.innerHTML = '<p class="empty">No named clients configured.</p>';
    } else {
        listEl.innerHTML = data.clients.map(client => {
            const grants = data.grants.filter(grant => grant.client_name === client.name);
            const grantsHtml = grants.length
                ? grants.map(grant => `
                    <div class="list-item">
                        <div>
                            <code>${escapeHtml(grant.origin)}</code>
                            <div class="muted">Granted: ${escapeHtml(formatUnix(grant.granted_at_unix))}</div>
                        </div>
                        <button onclick="revokeNamedClient(${JSON.stringify(client.name)}, ${JSON.stringify(grant.origin)})">Revoke</button>
                    </div>
                `).join('')
                : '<p class="empty">No grants yet.</p>';
            return `
                <div class="card">
                    <div class="list-item">
                        <div>
                            <strong>${escapeHtml(client.name)}</strong>
                            <div class="muted">Endpoint: <code>${escapeHtml(client.endpoint)}</code></div>
                            <div class="muted">Signer: <code>${escapeHtml(client.signer)}</code></div>
                        </div>
                        <button onclick="deleteNamedClient(${JSON.stringify(client.name)})">Delete</button>
                    </div>
                    <div class="section-title">Grants</div>
                    ${grantsHtml}
                    <div class="inline-row">
                        <input type="text" id="${grantInputId(client.name)}" placeholder="Origin or page URL, e.g. hppr://group/app/index.html or https://app.example/page">
                        <button onclick="grantNamedClient(${JSON.stringify(client.name)})">Grant Origin</button>
                    </div>
                </div>
            `;
        }).join('');
    }

    if (!data.revocations.length) {
        revocationsEl.innerHTML = '<p class="empty">None</p>';
    } else {
        revocationsEl.innerHTML = data.revocations.map(revocation => `
            <div class="list-item">
                <div>
                    <strong>${escapeHtml(revocation.client_name)}</strong>
                    <div class="muted"><code>${escapeHtml(revocation.origin)}</code></div>
                </div>
                <div class="muted">${escapeHtml(formatUnix(revocation.revoked_at_unix))}</div>
            </div>
        `).join('');
    }
}

async function loadNamedClients() {
    try {
        const data = await homeRepoRequest('named_clients');
        renderNamedClients(data);
    } catch (e) {
        const listEl = document.getElementById('namedClientsList');
        if (listEl) listEl.innerHTML = '<p class="error">' + escapeHtml(e instanceof Error ? e.message : String(e)) + '</p>';
    }
}

async function saveNamedClient() {
    const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('namedClientName'));
    const endpointEl = /** @type {HTMLInputElement|null} */ (document.getElementById('namedClientEndpoint'));
    const signerEl = /** @type {HTMLInputElement|null} */ (document.getElementById('namedClientSigner'));
    if (!nameEl || !endpointEl || !signerEl) return;

    const name = nameEl.value.trim();
    const endpoint = endpointEl.value.trim();
    const signer = signerEl.value.trim();
    if (!name || !endpoint || !signer) {
        setNamedClientMessage('Name, endpoint, and signer are required.', false);
        return;
    }

    try {
        await homeRepoRequest('set_named_client', { name, endpoint, signer });
        setNamedClientMessage('Named client saved.', true);
        await loadNamedClients();
    } catch (e) {
        setNamedClientMessage(e instanceof Error ? e.message : String(e), false);
    }
}

/** @param {string} name */
async function deleteNamedClient(name) {
    try {
        await homeRepoRequest('delete_named_client', { name });
        setNamedClientMessage('Named client deleted.', true);
        await loadNamedClients();
    } catch (e) {
        setNamedClientMessage(e instanceof Error ? e.message : String(e), false);
    }
}

/** @param {string} name */
async function grantNamedClient(name) {
    const input = /** @type {HTMLInputElement|null} */ (document.getElementById(grantInputId(name)));
    const origin = input?.value.trim() || '';
    if (!origin) {
        setNamedClientMessage('Grant origin or page URL is required.', false);
        return;
    }
    try {
        await homeRepoRequest('grant_named_client', { name, origin });
        if (input) input.value = '';
        setNamedClientMessage('Grant saved.', true);
        await loadNamedClients();
    } catch (e) {
        setNamedClientMessage(e instanceof Error ? e.message : String(e), false);
    }
}

/** @param {string} name @param {string} origin */
async function revokeNamedClient(name, origin) {
    try {
        await homeRepoRequest('revoke_named_client', { name, origin });
        setNamedClientMessage('Grant revoked.', true);
        await loadNamedClients();
    } catch (e) {
        setNamedClientMessage(e instanceof Error ? e.message : String(e), false);
    }
}

/** @type {{ saveRepoName?: () => Promise<void>, saveNamedClient?: () => Promise<void>, deleteNamedClient?: (name: string) => Promise<void>, grantNamedClient?: (name: string) => Promise<void>, revokeNamedClient?: (name: string, origin: string) => Promise<void> }} */
const g = /** @type {any} */ (window);
g.saveRepoName = saveRepoName;
g.saveNamedClient = saveNamedClient;
g.deleteNamedClient = deleteNamedClient;
g.grantNamedClient = grantNamedClient;
g.revokeNamedClient = revokeNamedClient;

loadStatus();
