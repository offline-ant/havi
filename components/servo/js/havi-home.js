// @ts-check
/// <reference path="havi.d.ts" />

const input = /** @type {HTMLInputElement|null} */ (document.getElementById('urlInput'));

if (input) {
    input.addEventListener('keydown', (e) => {
        if (e.key !== 'Enter') return;

        let value = input.value.trim();
        if (!value) return;

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
    });
}
