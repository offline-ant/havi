// @ts-check

(function() {
    const outputEl = document.getElementById('diagOutput');
    const fixtureEl = document.getElementById('joinFixtureState');
    const groupEl = document.getElementById('diagGroup');
    const appEl = document.getElementById('diagApp');
    const locationEl = document.getElementById('diagLocation');
    const messageEl = document.getElementById('message');

    if (!outputEl || !fixtureEl || !groupEl || !appEl || !locationEl || !messageEl) return;

    const output = /** @type {HTMLElement} */ (outputEl);
    const fixture = /** @type {HTMLElement} */ (fixtureEl);
    const groupInput = /** @type {HTMLInputElement} */ (groupEl);
    const appInput = /** @type {HTMLInputElement} */ (appEl);
    const locationInput = /** @type {HTMLInputElement} */ (locationEl);
    const message = /** @type {HTMLElement} */ (messageEl);

    /** @param {string} text @param {'error'|'success'} type */
    function showMessage(text, type) {
        message.innerHTML = '<div class="message ' + type + '">' + text + '</div>';
        setTimeout(() => {
            message.innerHTML = '';
        }, 4000);
    }

    /** @param {string} cmd @param {Record<string, string>=} params */
    async function diagnosticsRequest(cmd, params) {
        const query = new URLSearchParams({ cmd });
        if (params) {
            for (const [k, v] of Object.entries(params)) {
                query.set(k, String(v));
            }
        }
        const resp = await fetch('havi:///diagnostics/api?' + query.toString());
        const json = await resp.json();
        if (!json.ok) throw new Error(String(json.error || 'diagnostics error'));
        return json.data;
    }

    async function refreshJoinFixture() {
        try {
            const data = await diagnosticsRequest('join_fixture_get');
            fixture.textContent = String(data.state || 'none');
        } catch (e) {
            fixture.textContent = 'error';
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @param {'none'|'pending'|'approved'} state */
    async function setJoinFixture(state) {
        try {
            const data = await diagnosticsRequest('join_fixture_set', { state });
            fixture.textContent = String(data.state || state);
            showMessage('Join fixture set: ' + fixture.textContent, 'success');
        } catch (e) {
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    async function runDiagnostics() {
        const group = groupInput.value.trim();
        const app = appInput.value.trim();
        const location = locationInput.value.trim();

        if (!group || !app) {
            showMessage('group and app are required', 'error');
            return;
        }

        output.textContent = 'Inspecting...';

        try {
            const data = await diagnosticsRequest('inspect', { group, app, location });
            output.textContent = JSON.stringify(data, null, 2);
        } catch (e) {
            output.textContent = '';
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @type {{ runDiagnostics?: () => Promise<void>, setJoinFixture?: (state: 'none'|'pending'|'approved') => Promise<void> }} */
    const g = /** @type {any} */ (window);
    g.runDiagnostics = runDiagnostics;
    g.setJoinFixture = setJoinFixture;

    refreshJoinFixture();
})();
