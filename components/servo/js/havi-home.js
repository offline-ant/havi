// @ts-check
/// <reference path="havi.d.ts" />

const adminClient = window.havi?.admin?.client ?? null;
const input = /** @type {HTMLInputElement|null} */ (document.getElementById('urlInput'));

if (input) {
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            let value = input.value.trim();
            if (!value) return;

            // Convert URC to hppr:// URL
            if (value.startsWith('//')) {
                value = 'hppr:' + value;
            } else if (value.startsWith('-/')) {
                value = 'hppr:' + value;
            } else if (!value.includes('://') && !value.startsWith('hppr:')) {
                value = 'hppr://' + value;
            }

            if (!window.address) {
                throw new Error('window.address unavailable');
            }
            window.address.href = value;
        }
    });
}

// Load recent routes as quick links
async function loadQuickLinks() {
    try {
        if (!adminClient) return;
        const container = document.getElementById('quickLinks');
        if (!container) return;

        const greeting = await adminClient.hello();
        const adminKey = greeting.verifyingKey;
        if (!adminKey) return;

        const groups = await adminClient.list('//repo/route/app/');
        let count = 0;
        for (const group of groups) {
            if (count >= 5) break;
            const cleanGroup = group.replace(/\/$/, '');
            try {
                const apps = await adminClient.list('//repo/route/app/' + cleanGroup + '/');
                for (const app of apps) {
                    if (count >= 5) break;
                    const cleanApp = app.replace(/\/$/, '');
                    try {
                        const routeUrc = '//repo/route/app/' + cleanGroup + '/' + cleanApp + '/|/seal/' + adminKey;
                        await adminClient.get(routeUrc);
                        const link = document.createElement('a');
                        link.href = 'hppr://' + cleanGroup + '/' + cleanApp + '/';
                        link.className = 'quick-link';
                        link.textContent = '//' + cleanGroup + '/' + cleanApp;
                        container.appendChild(link);
                        count++;
                    } catch (_e) {
                        // No route for this app
                    }
                }
            } catch (_e) {
                // No apps in this group
            }
        }
    } catch (_e) {
        // Ignore errors loading routes
    }
}

loadQuickLinks();
