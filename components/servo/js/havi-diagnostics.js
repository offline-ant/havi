// @ts-check

(function() {
    const outputEl = document.getElementById('diagOutput');
    const groupEl = document.getElementById('diagGroup');
    const apiEl = document.getElementById('diagApi');
    const keyEl = document.getElementById('diagKey');
    const messageEl = document.getElementById('message');

    if (!outputEl || !groupEl || !apiEl || !keyEl || !messageEl) return;

    const output = /** @type {HTMLElement} */ (outputEl);
    const groupInput = /** @type {HTMLInputElement} */ (groupEl);
    const apiInput = /** @type {HTMLInputElement} */ (apiEl);
    const keyInput = /** @type {HTMLInputElement} */ (keyEl);
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

    async function runDiagnostics() {
        const group = groupInput.value.trim();
        const api = apiInput.value.trim();
        const key = keyInput.value.trim();

        if (!group || !api) {
            showMessage('group and api are required', 'error');
            return;
        }

        output.textContent = 'Inspecting...';

        try {
            const data = await diagnosticsRequest('inspect', { group, api, key });
            output.textContent = JSON.stringify(data, null, 2);
        } catch (e) {
            output.textContent = '';
            showMessage(e instanceof Error ? e.message : String(e), 'error');
        }
    }

    /** @type {{ runDiagnostics?: () => Promise<void> }} */
    const g = /** @type {any} */ (window);
    g.runDiagnostics = runDiagnostics;
})();
