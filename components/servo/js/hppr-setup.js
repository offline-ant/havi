// @ts-check
/// <reference path="havi.d.ts" />

// Globals injected by the Rust template.
/** @type {string} */ var GROUP;
/** @type {string} */ var APP;
/** @type {string} */ var ENDPOINT;
/** @type {string} */ var LOCATION;

/** @type {{ repoName: string|null, sessionId: string|null, verifyingKey: string|null }|null} */
let greeting = null;
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

        const anyoneClient = await HpprClient.connect(ENDPOINT);
        const remoteGreeting = await anyoneClient.hello();
        greeting = remoteGreeting;

        if (!remoteGreeting.verifyingKey || remoteGreeting.verifyingKey === '0') {
            showError('Repo has no verification key. Cannot install route.');
            return;
        }

        const repoIdEl = document.getElementById('repo-id');
        if (repoIdEl) repoIdEl.textContent = remoteGreeting.repoName || '(none)';
        const repoKeyEl = document.getElementById('repo-key');
        if (repoKeyEl) repoKeyEl.textContent = remoteGreeting.verifyingKey;

        try {
            const localGreeting = await ring0.hello();
            const localKey = localGreeting.verifyingKey;
            if (localKey) {
                const localRouteUrc = '//repo/admin/route/' + GROUP + '/' + APP + '/|/seal/' + localKey;
                const localPacket = await ring0.get(localRouteUrc);
                localRoute = {
                    endpoint: localPacket.getHeader('Upstream')
                };
            }
        } catch (e) {
            console.log('No home repo route:', e);
        }

        if (localRoute) {
            const localStateEl = document.getElementById('local-state');
            if (localStateEl) localStateEl.style.display = 'block';
            const localEndpointEl = document.getElementById('local-endpoint');
            if (localEndpointEl) localEndpointEl.textContent = localRoute.endpoint || '(not set)';
            setDiffIndicator('endpoint-diff', false, localRoute.endpoint !== ENDPOINT);
        } else {
            setDiffIndicator('endpoint-diff', true, false);
        }

        const appPart = APP ? '/' + APP : '';
        const previewLocation = LOCATION || 'index.html';
        const previewUrl = 'hppr-sandbox://' + GROUP + appPart + '/' + previewLocation + '{via:' + ENDPOINT + '}';
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

        const setEndpoint = /** @type {HTMLInputElement|null} */ (document.getElementById('set-endpoint'))?.checked ?? true;

        if (!ring0) throw new Error('ring0 unavailable');

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

        const appPart = APP ? '/' + APP : '';
        window.address.href = 'hppr://' + GROUP + appPart + '/';
    } catch (e) {
        showError('Failed to save route: ' + (e instanceof Error ? e.message : String(e)));
        if (btn) {
            btn.disabled = false;
            btn.textContent = 'Accept & Save Route';
        }
    }
}

function cancel() {
    if (history.length > 1) {
        history.back();
    } else {
        window.address.href = 'havi:///overview';
    }
}

init();
