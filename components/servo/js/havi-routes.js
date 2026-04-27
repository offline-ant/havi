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

const adminClient = window.havi?.admin?.client ?? null;

/** @type {string|null} */
let localAdminKey = null;

async function getLocalAdminKey() {
    if (localAdminKey) return localAdminKey;
    try {
        if (!adminClient) return null;
        const greeting = await adminClient.hello();
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
        if (!adminKey || !adminClient) {
            list.innerHTML = '<p class="empty">Unable to get local admin key</p>';
            return;
        }

        const groups = await adminClient.list('//repo/route/app/');
        /** @type {{ group: string, app: string, packet: HpprPacket }[]} */
        const routes = [];

        for (const group of groups) {
            const cleanGroup = group.replace(/\/$/, '');
            try {
                const apps = await adminClient.list('//repo/route/app/' + cleanGroup + '/');
                for (const app of apps) {
                    const cleanApp = app.replace(/\/$/, '');
                    try {
                        const routeUrc = '//repo/route/app/' + cleanGroup + '/' + cleanApp + '/|/seal/' + adminKey;
                        const packet = await adminClient.get(routeUrc);
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
        if (!adminClient) throw new Error('window.havi.admin.client unavailable');
        await adminClient.detach(hash);
        showMessage('Removed route: //' + group + '/' + app, false);
        loadRoutes();
    } catch (e) {
        showMessage('Failed to remove route: ' + (e instanceof Error ? e.message : String(e)), true);
    }
}

loadRoutes();
