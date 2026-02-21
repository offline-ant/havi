// @ts-check
/// <reference path="havi.d.ts" />

async function loadStatus() {
    try {
        if (!window.ring0 || !window.ring0.repo) throw new Error('ring0.repo unavailable');
        const port = await window.ring0.repo.port();
        const portEl = document.getElementById('port');
        if (portEl) portEl.textContent = String(port);


        const wsPortEl = document.getElementById('wsPort');
        if (wsPortEl) wsPortEl.textContent = String(port + 1);

        const quibPortEl = document.getElementById('quibPort');
        if (quibPortEl) quibPortEl.textContent = port > 0 ? String(port - 1) : '-';

        const udpPortEl = document.getElementById('udpPort');
        if (udpPortEl) udpPortEl.textContent = port > 0 ? String(port) : '-';

        const statusEl = document.getElementById('status');
        if (statusEl) statusEl.textContent = status;

        const repoPath = await window.ring0.repo.repoPath();
        const repoEl = document.getElementById('repoPath');
        if (repoEl) repoEl.textContent = repoPath || '(not available)';
    } catch (e) {
        console.error('Failed to load repo status:', e);
        const portEl = document.getElementById('port');
        if (portEl) portEl.textContent = 'Error';
        const statusEl = document.getElementById('status');
        if (statusEl) statusEl.textContent = 'Error';
    }

    // Load repo verification key and daemon info via HELLO
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const greeting = await window.ring0.hello();
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

    // Load current repo name from identity config
    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const identity = await window.ring0.get('//repo/admin/identity/|');
        const repoName = identity.getHeader('Repo-Name');
        const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
        if (nameEl) nameEl.value = repoName || 'localhost';
    } catch (_e) {
        const nameEl = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
        if (nameEl) nameEl.placeholder = '(error loading)';
    }
}

async function saveRepoName() {
    const nameInput = /** @type {HTMLInputElement|null} */ (document.getElementById('repoName'));
    const msgEl = document.getElementById('nameMessage');
    if (!nameInput || !msgEl) return;
    const newName = nameInput.value.trim();

    if (!newName) {
        msgEl.textContent = 'Please enter a repo name';
        msgEl.style.color = '#ff6b6b';
        return;
    }

    try {
        if (!window.ring0) throw new Error('ring0 unavailable');
        const identity = await window.ring0.get('//repo/admin/identity/|');
        const newHeaders = identity.customHeaders()
            .filter(/** @param {string} h */ h => !h.startsWith('Repo-Name:'));
        newHeaders.push('Repo-Name: ' + newName);

        await window.ring0.add({
            headers: newHeaders,
            data: identity.text()
        });

        msgEl.textContent = 'Repo name updated. Restart repo daemon to take effect.';
        msgEl.style.color = '#27ae60';
    } catch (e) {
        msgEl.textContent = 'Failed to save: ' + (e instanceof Error ? e.message : String(e));
        msgEl.style.color = '#ff6b6b';
    }
}

loadStatus();
