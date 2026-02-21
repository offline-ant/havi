// @ts-check
/// <reference path="havi.d.ts" />

// Globals injected by the Rust template.
// The Rust handler replaces these with html_escape'd values.
/** @type {string} */ var GROUP;
/** @type {string} */ var APP;
/** @type {string} */ var ENDPOINT;
/** @type {string} */ var LOCATION;

/** @type {{ repoName: string|null, sessionId: string|null, verifyingKey: string|null }|null} */
let greeting = null;
/** @type {string|null} */
let siteTrustKey = null;
/** @type {{ endpoint: string|null }|null} */
let localRoute = null;

const ring0 = window.ring0;

/** @param {string} msg */
function showError(msg) {
    const loadingEl = document.getElementById('loading');
    if (loadingEl) loadingEl.style.display = 'none';
    const errorEl = document.getElementById('error');
    if (errorEl) {
        errorEl.textContent = msg;
        errorEl.style.display = 'block';
    }
}

/**
 * @param {string} elementId
 * @param {boolean} isNew
 * @param {boolean} isChanged
 */
function setDiffIndicator(elementId, isNew, isChanged) {
    const el = document.getElementById(elementId);
    if (!el) return;
    if (isNew) {
        el.textContent = 'new';
        el.className = 'diff-indicator diff-new';
    } else if (isChanged) {
        el.textContent = 'changed';
        el.className = 'diff-indicator diff-changed';
    } else {
        el.textContent = 'same';
        el.className = 'diff-indicator diff-same';
    }
}

async function init() {
    try {
        if (!ring0) {
            showError('window.ring0 unavailable: admin credentials not pre-fetched');
            return;
        }
        // 1. Create anyone client and get remote repo hello to get their verification key
        const anyoneClient = await HpprClient.connect(ENDPOINT);
        const remoteGreeting = await anyoneClient.hello();
        greeting = remoteGreeting;

        if (!remoteGreeting.verifyingKey || remoteGreeting.verifyingKey === '0') {
            showError('Repo has no verification key. Cannot establish trust.');
            return;
        }

        const repoIdEl = document.getElementById('repo-id');
        if (repoIdEl) repoIdEl.textContent = remoteGreeting.repoName || '(none)';
        const repoKeyEl = document.getElementById('repo-key');
        if (repoKeyEl) repoKeyEl.textContent = remoteGreeting.verifyingKey;

        // 2. Fetch remote site-trust via MEMBERS
        /** @type {string[]} */
        let remoteTrustKeys = [];
        try {
            const appPart = APP ? '/' + APP : '';
            const siteTrustUrc = '//' + GROUP + appPart + '/site-trust/|/seal/' + remoteGreeting.verifyingKey;
            const result = await anyoneClient.members(siteTrustUrc);
            remoteTrustKeys = (result || []).map(/** @param {string} line */ line => line.split(' ')[0]);
        } catch (e) {
            console.log('No remote site-trust:', e);
        }
        siteTrustKey = remoteTrustKeys[0] || remoteGreeting.verifyingKey;

        // 3. Fetch home repo route and site-trust (if exists)
        /** @type {string|null} */
        let localKey = null;
        try {
            const localGreeting = await ring0.hello();
            localKey = localGreeting.verifyingKey;
            if (localKey) {
                const appPart = APP ? '/' + APP : '';
                const localRouteUrc = '//repo/admin/route/' + GROUP + '/' + APP + '/|/seal/' + localKey;
                const localPacket = await ring0.get(localRouteUrc);
                localRoute = {
                    endpoint: localPacket.getHeader('Upstream')
                };
            }
        } catch (e) {
            console.log('No home repo route:', e);
        }

        /** @type {string[]} */
        let localTrustKeys = [];
        if (localKey) {
            try {
                const appPart = APP ? '/' + APP : '';
                const siteTrustUrc = '//' + GROUP + appPart + '/site-trust/|/seal/' + localKey;
                const result = await ring0.members(siteTrustUrc);
                localTrustKeys = (result || []).map(/** @param {string} line */ line => line.split(' ')[0]);
            } catch (e) {
                console.log('No local site-trust:', e);
            }
        }

        // 4. Update UI
        /** @param {string} k */
        const truncKey = (k) => k.length > 32 ? k.substring(0, 10) + '...' + k.substring(k.length - 6) : k;

        const remoteTrustEl = document.getElementById('remote-trust-keys');
        if (remoteTrustEl) {
            remoteTrustEl.textContent = remoteTrustKeys.length > 0
                ? remoteTrustKeys.map(truncKey).join(', ')
                : '(repo key)';
        }

        if (localRoute) {
            const localStateEl = document.getElementById('local-state');
            if (localStateEl) localStateEl.style.display = 'block';
            const localEndpointEl = document.getElementById('local-endpoint');
            if (localEndpointEl) localEndpointEl.textContent = localRoute.endpoint || '(not set)';
            const localTrustEl = document.getElementById('local-trust-keys');
            if (localTrustEl) {
                localTrustEl.textContent = localTrustKeys.length > 0
                    ? localTrustKeys.map(truncKey).join(', ')
                    : '(not set)';
            }
            const endpointChanged = localRoute.endpoint !== ENDPOINT;
            const trustChanged = JSON.stringify(localTrustKeys) !== JSON.stringify(remoteTrustKeys.length > 0 ? remoteTrustKeys : [siteTrustKey]);
            setDiffIndicator('endpoint-diff', false, endpointChanged);
            setDiffIndicator('trust-diff', false, trustChanged);
        } else {
            setDiffIndicator('endpoint-diff', true, false);
            setDiffIndicator('trust-diff', true, false);
        }

        // 6. Set up preview xframe with seal verification
        const appPart = APP ? '/' + APP : '';
        const previewLocation = LOCATION || 'index.html';
        const previewUrl = 'hppr-sandbox://' + GROUP + appPart + '/' + previewLocation + '/|/seal/' + siteTrustKey + '{via:' + ENDPOINT + '}';
        const previewFrame = document.getElementById('preview-frame');
        if (previewFrame) previewFrame.setAttribute('src', previewUrl);

        const loadingEl = document.getElementById('loading');
        if (loadingEl) loadingEl.style.display = 'none';
        const contentEl = document.getElementById('content');
        if (contentEl) contentEl.style.display = 'block';
    } catch (e) {
        showError('Failed to connect: ' + (e instanceof Error ? e.message : String(e)));
    }
}

async function accept() {
    const btn = /** @type {HTMLButtonElement|null} */ (document.getElementById('accept-btn'));
    if (btn) {
        btn.disabled = true;
        btn.textContent = 'Saving...';
    }

    try {
        if (!greeting) throw new Error('No greeting received');

        const adoptAdminKey = /** @type {HTMLInputElement|null} */ (document.getElementById('adopt-admin-key'))?.checked ?? true;
        const setEndpoint = /** @type {HTMLInputElement|null} */ (document.getElementById('set-endpoint'))?.checked ?? true;

        if (!ring0) throw new Error('ring0 unavailable');
        const localGreeting = await ring0.hello();
        const localKey = localGreeting.verifyingKey;

        if (!localKey) {
            throw new Error('Home repo has no verification key');
        }

        if (setEndpoint) {
            const routeHeaders = [
                'Seal-By: oldest',
                'Group: repo',
                'App: admin',
                'Location: route/' + GROUP + '/' + APP,
                'Upstream: ' + ENDPOINT,
                'Upstream-Verification-Key: ' + greeting.verifyingKey
            ];
            await ring0.add({ headers: routeHeaders, data: '' });
        }

        if (adoptAdminKey) {
            const siteTrustHeaders = [
                'Seal-By: ' + localKey,
                'Group: ' + GROUP,
                'App: ' + (APP || ''),
                'Location: site-trust',
                'Member: ' + siteTrustKey
            ];
            await ring0.add({ headers: siteTrustHeaders, data: '' });
        }

        const appPart = APP ? '/' + APP : '';
        window.location.href = 'hppr://' + GROUP + appPart + '/';
    } catch (e) {
        showError('Failed to save trust: ' + (e instanceof Error ? e.message : String(e)));
        if (btn) {
            btn.disabled = false;
            btn.textContent = 'Accept & Trust';
        }
    }
}

function cancel() {
    if (history.length > 1) {
        history.back();
    } else {
        window.location.href = 'havi:///overview';
    }
}

init();
