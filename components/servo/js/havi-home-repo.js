// @ts-check
/// <reference path="havi.d.ts" />

const haviAdmin = window.havi?.admin ?? null;
const adminClient = haviAdmin?.client ?? null;
const repoInfo = haviAdmin?.repo ?? null;

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

async function loadStatus() {
    try {
        if (!repoInfo) throw new Error('window.havi.admin.repo unavailable');
        const port = await repoInfo.port();
        const portEl = document.getElementById('port');
        if (portEl) portEl.textContent = String(port);

        const wsPortEl = document.getElementById('wsPort');
        if (wsPortEl) wsPortEl.textContent = String(port + 1);

        const quibPortEl = document.getElementById('quibPort');
        if (quibPortEl) quibPortEl.textContent = port > 0 ? String(port - 1) : '-';

        const udpPortEl = document.getElementById('udpPort');
        if (udpPortEl) udpPortEl.textContent = port > 0 ? String(port) : '-';

        const status = await repoInfo.status();
        const statusEl = document.getElementById('status');
        if (statusEl) statusEl.textContent = status;

        const repoPath = await repoInfo.repoPath();
        const repoEl = document.getElementById('repoPath');
        if (repoEl) repoEl.textContent = repoPath || '(not available)';
    } catch (e) {
        console.error('Failed to load repo status:', e);
        const portEl = document.getElementById('port');
        if (portEl) portEl.textContent = 'Error';
        const statusEl = document.getElementById('status');
        if (statusEl) statusEl.textContent = 'Error';
    }

    try {
        if (!adminClient) throw new Error('window.havi.admin.client unavailable');
        const greeting = await adminClient.hello();
        const keyEl = document.getElementById('repoKey');
        if (keyEl) keyEl.textContent = greeting.verifyingKey || '(none)';

        if (greeting.status || greeting.uptime || greeting.version || greeting.backend) {
            const card = document.getElementById('daemonInfoCard');
            if (card) card.style.display = '';

            const statusEl = document.getElementById('daemonStatus');
            if (statusEl) statusEl.textContent = greeting.status || '-';

            const uptimeEl = document.getElementById('daemonUptime');
            if (uptimeEl) {
                const secs = parseInt(greeting.uptime || '0', 10);
                if (secs >= 86400) {
                    uptimeEl.textContent = Math.floor(secs / 86400) + 'd ' + Math.floor((secs % 86400) / 3600) + 'h';
                } else if (secs >= 3600) {
                    uptimeEl.textContent = Math.floor(secs / 3600) + 'h ' + Math.floor((secs % 3600) / 60) + 'm';
                } else if (secs >= 60) {
                    uptimeEl.textContent = Math.floor(secs / 60) + 'm ' + (secs % 60) + 's';
                } else {
                    uptimeEl.textContent = secs + 's';
                }
            }

            const backendEl = document.getElementById('daemonBackend');
            if (backendEl) backendEl.textContent = greeting.backend || '-';

            const versionEl = document.getElementById('daemonVersion');
            if (versionEl) versionEl.textContent = greeting.version || '-';
        }
    } catch (e) {
        const keyEl = document.getElementById('repoKey');
        if (keyEl) keyEl.textContent = '(error: ' + (e instanceof Error ? e.message : String(e)) + ')';
    }

    try {
        if (!adminClient) throw new Error('window.havi.admin.client unavailable');
        const identity = await adminClient.get('//repo/admin/identity/|');
        const repoName = identity.getHeader('Repo-Name');
        const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
        if (nameEl) nameEl.value = repoName || 'localhost';
    } catch (_e) {
        const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
        if (nameEl) nameEl.placeholder = '(error loading)';
    }

    await loadNamedClients();
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
        if (!adminClient) throw new Error('window.havi.admin.client unavailable');
        const identity = await adminClient.get('//repo/admin/identity/|');
        const newHeaders = identity.customHeaders()
            .filter(/** @param {string} h */ h => !h.startsWith('Repo-Name:'));
        newHeaders.push('Repo-Name: ' + newName);

        await adminClient.add({
            headers: newHeaders,
            data: identity.text()
        });

        msgEl.textContent = 'Repo name updated. Restart repo daemon to take effect.';
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
