// ═══════════════════════════════════════════════════════════
// Argos Search — Launcher Mode JS
// Keyboard-driven: type → results → ↑↓ navigate → Enter open → Esc hide
// ═══════════════════════════════════════════════════════════

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const input = document.getElementById('launcher-input');
const resultsContainer = document.getElementById('launcher-results');
const hint = document.getElementById('launcher-hint');

let debounceTimer = null;
let results = [];
let selectedIndex = -1;

// ─── Focus on show ──────────────────────────────────────────
const appWindow = getCurrentWindow();
appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) {
        input.focus();
        input.select();
    } else {
        // Auto-hide when loses focus
        hideLauncher();
    }
});

// ─── Input: live search ─────────────────────────────────────
input.addEventListener('input', () => {
    clearTimeout(debounceTimer);
    const q = input.value.trim();
    if (q.length < 2) {
        hideResults();
        return;
    }
    debounceTimer = setTimeout(() => doSearch(q), 150);
});

async function doSearch(query) {
    try {
        const r = await invoke('search', { request: { query, limit: 8 } });
        results = r.hits;
        selectedIndex = results.length > 0 ? 0 : -1;
        renderResults();
    } catch (e) {
        console.error('Launcher search error:', e);
    }
}

// ─── Render ─────────────────────────────────────────────────
function renderResults() {
    if (results.length === 0) {
        hideResults();
        return;
    }

    resultsContainer.innerHTML = '';
    resultsContainer.classList.remove('hidden');

    // Resize window to fit results
    const itemHeight = 50;
    const padding = 12 + 6; // bar padding + results margin
    const barHeight = 56;
    const resultsHeight = Math.min(results.length * itemHeight + 12, 372);
    const totalHeight = barHeight + resultsHeight + padding;
    appWindow.setSize(new window.__TAURI__.window.LogicalSize(680, totalHeight));

    results.forEach((hit, i) => {
        const item = document.createElement('div');
        item.className = `launcher-item${i === selectedIndex ? ' selected' : ''}`;
        item.dataset.index = i;

        const ext = getExtension(hit.name);
        const shortPath = shortenPath(hit.path);

        item.innerHTML = `
            <div class="launcher-item-ext">${escapeHtml(ext.substring(0, 4))}</div>
            <div class="launcher-item-info">
                <div class="launcher-item-name">${escapeHtml(hit.name)}</div>
                <div class="launcher-item-path">${escapeHtml(shortPath)}</div>
            </div>
            <div class="launcher-item-action">Enter ↵</div>`;

        item.addEventListener('click', () => openResult(i));
        item.addEventListener('mouseenter', () => {
            selectedIndex = i;
            updateSelection();
        });

        resultsContainer.appendChild(item);
    });

    hint.textContent = `${results.length} results`;
}

function hideResults() {
    results = [];
    selectedIndex = -1;
    resultsContainer.innerHTML = '';
    resultsContainer.classList.add('hidden');
    hint.textContent = 'ESC to close';
    // Reset window size to bar only
    appWindow.setSize(new window.__TAURI__.window.LogicalSize(680, 72));
}

function updateSelection() {
    const items = resultsContainer.querySelectorAll('.launcher-item');
    items.forEach((item, i) => {
        item.classList.toggle('selected', i === selectedIndex);
    });
}

// ─── Keyboard Navigation ────────────────────────────────────
document.addEventListener('keydown', async (e) => {
    if (e.key === 'Escape') {
        e.preventDefault();
        hideLauncher();
        return;
    }

    if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (results.length > 0) {
            selectedIndex = (selectedIndex + 1) % results.length;
            updateSelection();
            scrollToSelected();
        }
        return;
    }

    if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (results.length > 0) {
            selectedIndex = selectedIndex <= 0 ? results.length - 1 : selectedIndex - 1;
            updateSelection();
            scrollToSelected();
        }
        return;
    }

    if (e.key === 'Enter') {
        e.preventDefault();
        if (selectedIndex >= 0 && selectedIndex < results.length) {
            openResult(selectedIndex);
        }
        return;
    }
});

function scrollToSelected() {
    const selected = resultsContainer.querySelector('.launcher-item.selected');
    if (selected) selected.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
}

// ─── Actions ────────────────────────────────────────────────
async function openResult(index) {
    const hit = results[index];
    if (!hit) return;
    try {
        await invoke('open_file', { path: hit.path });
    } catch (e) {
        console.error('Open error:', e);
    }
    hideLauncher();
}

async function hideLauncher() {
    input.value = '';
    hideResults();
    try {
        await invoke('hide_launcher_window');
    } catch (e) {
        console.error('Hide error:', e);
    }
}

// ─── Helpers ────────────────────────────────────────────────
function getExtension(name) {
    const parts = name.split('.');
    return parts.length > 1 ? parts.pop().toLowerCase() : '?';
}

function shortenPath(p) {
    p = p.replace(/^\\\\\?\\/, '');
    const winMatch = p.match(/^([A-Z]:\\Users\\[^\\]+\\)/i);
    const macMatch = p.match(/^(\/Users\/[^/]+\/)/);
    const homePrefix = winMatch ? winMatch[1] : macMatch ? macMatch[1] : null;
    if (homePrefix) {
        p = '~' + (winMatch ? '\\' : '/') + p.substring(homePrefix.length);
    }
    return p;
}

function escapeHtml(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
}

// ─── Init ───────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
    input.focus();
});
