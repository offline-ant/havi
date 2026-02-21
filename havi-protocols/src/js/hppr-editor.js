// @ts-check
/// <reference path="havi.d.ts" />

async function saveContent() {
    const headersEl = /** @type {HTMLTextAreaElement|null} */ (document.getElementById('headers'));
    const dataEl = /** @type {HTMLTextAreaElement|null} */ (document.getElementById('data'));
    const status = document.getElementById('status');
    const saveBtn = /** @type {HTMLButtonElement|null} */ (document.getElementById('saveBtn'));

    if (!headersEl || !dataEl || !status || !saveBtn) return;

    saveBtn.disabled = true;

    const headersText = headersEl.value;
    const data = dataEl.value;
    const headerLines = headersText.split(/\r?\n/).filter(line => line.trim());

    status.className = 'status saving';
    status.textContent = 'Saving...';

    try {
        console.log('[hppr-editor] Saving via ring0, headers:', JSON.stringify(headerLines));

        if (!window.ring0) throw new Error('ring0 unavailable');
        await window.ring0.add({
            headers: headerLines,
            data: data
        });

        console.log('[hppr-editor] Save OK');

        // Parse headers to build redirect URL
        let group = '', app = '', location = '';
        for (const line of headersText.split(/\r?\n/)) {
            if (line.startsWith('Group: ')) group = line.slice(7).trim();
            else if (line.startsWith('App: ')) app = line.slice(5).trim();
            else if (line.startsWith('Location: ')) location = line.slice(10).trim();
        }

        const redirect = location
            ? `hppr://${group}/${app}/${location}`
            : `hppr://${group}/${app}`;

        console.log('[hppr-editor] Redirecting to', redirect);
        window.open(redirect, '_self');
    } catch (e) {
        console.error('[hppr-editor] Save failed:', e instanceof Error ? e.message : e);
        status.className = 'status error';
        status.textContent = e instanceof Error ? e.message : 'Save failed';
        saveBtn.disabled = false;
    }
}

// Ctrl+S shortcut
document.addEventListener('keydown', function(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        saveContent();
    }
});

// Tab handling in textareas
/** @param {KeyboardEvent} e */
function handleTab(e) {
    if (e.key === 'Tab') {
        e.preventDefault();
        const target = /** @type {HTMLTextAreaElement} */ (e.target);
        const start = target.selectionStart;
        const end = target.selectionEnd;
        const value = target.value;

        if (e.shiftKey) {
            const lineStart = value.lastIndexOf('\n', start - 1) + 1;
            if (value.substring(lineStart, lineStart + 4) === '    ') {
                target.value = value.substring(0, lineStart) + value.substring(lineStart + 4);
                target.selectionStart = target.selectionEnd = start - 4;
            }
        } else {
            target.value = value.substring(0, start) + '    ' + value.substring(end);
            target.selectionStart = target.selectionEnd = start + 4;
        }
    }
}

const headersTextarea = document.getElementById('headers');
if (headersTextarea) headersTextarea.addEventListener('keydown', handleTab);
const dataTextarea = document.getElementById('data');
if (dataTextarea) dataTextarea.addEventListener('keydown', handleTab);
