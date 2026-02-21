// @ts-check
/// <reference path="havi.d.ts" />

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

            window.location.href = value;
        }
    });
}

// Load recent routes as quick links
async function loadQuickLinks() {
    try {
        if (!window.ring0) return;
        const container = document.getElementById('quickLinks');
        if (!container) return;

        const greeting = await window.ring0.hello();
        const adminKey = greeting.verifyingKey;
        if (!adminKey) return;

        const groups = await window.ring0.list('//repo/admin/route/');
        let count = 0;
        for (const group of groups) {
            if (count >= 5) break;
            const cleanGroup = group.replace(/\/$/, '');
            try {
                const apps = await window.ring0.list('//repo/admin/route/' + cleanGroup + '/');
                for (const app of apps) {
                    if (count >= 5) break;
                    const cleanApp = app.replace(/\/$/, '');
                    try {
                        const routeUrc = '//repo/admin/route/' + cleanGroup + '/' + cleanApp + '/|/seal/' + adminKey;
                        await window.ring0.get(routeUrc);
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
