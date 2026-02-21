// @ts-check
/// <reference path="havi.d.ts" />

/** @param {string} text @param {boolean} isError */
function showMessage(text, isError) {
    const el = document.getElementById('message');
    if (!el) return;
    el.className = 'message ' + (isError ? 'error' : 'success');
    el.textContent = text;
    el.style.display = 'block';
    setTimeout(() => el.style.display = 'none', 5000);
}

/** @param {string} key */
function truncateKey(key) {
    if (!key || key.length < 20) return key;
    return key.substring(0, 10) + '...' + key.substring(key.length - 6);
}

/** @type {string|null} */
let localAdminKey = null;

async function getLocalAdminKey() {
    if (localAdminKey) return localAdminKey;
    try {
        if (!window.ring0) return null;
        const greeting = await window.ring0.hello();
        if (greeting.verifyingKey) {
            localAdminKey = greeting.verifyingKey;
            return localAdminKey;
        }
    } catch (e) {
        console.error('Failed to get admin key:', e);
    }
    return null;
}

async function loadRoutes() {
    const list = document.getElementById('routesList');
    if (!list) return;
    try {
        const adminKey = await getLocalAdminKey();
        if (!adminKey || !window.ring0) {
            list.innerHTML = '<p class="empty">Unable to get local admin key</p>';
            return;
        }

        const groups = await window.ring0.list('//repo/admin/route/');
        /** @type {{ group: string, app: string, packet: HpprPacket }[]} */
        const routes = [];

        for (const group of groups) {
            const cleanGroup = group.replace(/\/$/, '');
            try {
                const apps = await window.ring0.list('//repo/admin/route/' + cleanGroup + '/');
                for (const app of apps) {
                    const cleanApp = app.replace(/\/$/, '');
                    try {
                        const routeUrc = '//repo/admin/route/' + cleanGroup + '/' + cleanApp + '/|/seal/' + adminKey;
                        const packet = await window.ring0.get(routeUrc);
                        routes.push({ group: cleanGroup, app: cleanApp, packet });
                    } catch (_e) {
                        // No route for this app
                    }
                }
            } catch (_e) {
                // No apps in this group
            }
        }

        if (routes.length === 0) {
            list.innerHTML = '<p class="empty">No routes configured - all traffic goes to localhost</p>';
            return;
        }

        let html = '';
        for (const route of routes) {
            const hdrs = route.packet.headers();
            const key = route.group + '/' + route.app;

            const upstreamAddress = hdrs.find(h => h.toLowerCase().startsWith('upstream:'));
            const upstreamKey = hdrs.find(h => h.toLowerCase().startsWith('upstream-verification-key:'));

            const upstreamAddressVal = upstreamAddress ? upstreamAddress.split(':').slice(1).join(':').trim() : '(unknown)';
            const upstreamKeyVal = upstreamKey ? upstreamKey.split(':').slice(1).join(':').trim() : '(none)';

            // Load site-trust members for this coordinate
            /** @type {string[]} */
            let trustedKeys = [];
            try {
                const siteTrustUrc = '//' + route.group + '/' + route.app + '/site-trust/|/seal/' + adminKey;
                const result = await window.ring0.members(siteTrustUrc);
                trustedKeys = (result || []).map(/** @param {string} line */ line => line.split(' ')[0]);
            } catch (_e) {
                // No site-trust
            }

            html += `
                <div class="route-item">
                    <div class="route-header">
                        <span class="route-title">//${key}</span>
                        <div>
                            <button class="danger" onclick="removeRoute('${route.group}', '${route.app}', '${route.packet.hash}')">Remove</button>
                        </div>
                    </div>
                    <div class="route-detail">
                        <span class="route-label">Upstream:</span>
                        <span class="route-value">${upstreamAddressVal}</span>
                    </div>
                    <div class="route-detail">
                        <span class="route-label">Upstream-Key:</span>
                        <span class="route-value trusted-key" title="${upstreamKeyVal}">
                            ${truncateKey(upstreamKeyVal)}
                        </span>
                    </div>
                    <div class="route-detail">
                        <span class="route-label">Site-Trust:</span>
                        <span class="route-value trusted-key">
                            ${trustedKeys.length > 0 ? trustedKeys.map(k => truncateKey(k)).join(', ') : '(none)'}
                        </span>
                    </div>
                </div>
            `;
        }

        list.innerHTML = html;
    } catch (_e) {
        if (list) list.innerHTML = '<p class="empty">No routes configured - all traffic goes to localhost</p>';
    }
}

/**
 * @param {string} group
 * @param {string} app
 * @param {string} hash
 */
async function removeRoute(group, app, hash) {
    if (!confirm('Remove route for "//' + group + '/' + app + '"?')) return;
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        await window.ring0.detach(hash);
        showMessage('Removed route: //' + group + '/' + app, false);
        loadRoutes();
    } catch (e) {
        showMessage('Failed to remove route: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

loadRoutes();
