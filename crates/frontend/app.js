// ═══════════════════════════════════════════════════════════
// Argos Search — Frontend v0.4
// System tray, global shortcut, search history
// ═══════════════════════════════════════════════════════════

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ─── Search History (localStorage) ──────────────────────────
const HISTORY_KEY = 'argos_search_history';
const MAX_HISTORY = 20;

function getHistory() {
    try { return JSON.parse(localStorage.getItem(HISTORY_KEY) || '[]'); }
    catch { return []; }
}
function addToHistory(query) {
    if (!query || query.length < 2) return;
    let h = getHistory().filter(q => q !== query);
    h.unshift(query);
    if (h.length > MAX_HISTORY) h = h.slice(0, MAX_HISTORY);
    localStorage.setItem(HISTORY_KEY, JSON.stringify(h));
}
function clearHistory() { localStorage.removeItem(HISTORY_KEY); }

// ─── DOM ────────────────────────────────────────────────────
const $ = id => document.getElementById(id);
const btnFolder    = $('btn-folder');
const btnIndex     = $('btn-index');
const btnSettings  = $('btn-settings');
const searchInput  = $('search-input');
const fileCount    = $('file-count');
const rootBar      = $('root-bar');
const rootPath     = $('root-path');
const statusText   = $('status-text');
const searchStats  = $('search-stats');
const resultsList  = $('results-list');
const emptyState   = $('empty-state');
const noResults    = $('no-results');
const scanOverlay  = $('scan-overlay');
const scanDetail   = $('scan-detail');
const contextMenu  = $('context-menu');
const settingsPanel = $('settings-panel');
const launchModeSelect = $('launch-mode-select');
const scopeBtns    = document.querySelectorAll('.scope-btn');

let debounceTimer = null;
let isIndexing = false;
let contextTarget = null; // path of right-clicked result

// ─── Init ───────────────────────────────────────────────────
async function initApp() {
    setStatus('⏳ Initializing...');
    try {
        const r = await invoke('init_engine');
        if (r.ready) {
            showRoots(r);
            activateScopeBtn(r.scope);
            btnIndex.disabled = false;
            searchInput.disabled = false;
            updateFileCount(r.indexed_count);
            if (r.indexed_count === 0) setStatus('👁️ Ready — Click ⚡ Index');
            else { setStatus(`✅ ${r.message} — ${r.indexed_count.toLocaleString()} files`); searchInput.focus(); }
        } else setStatus(`❌ ${r.message}`);

        // Load custom roots
        await refreshCustomRoots();

        // Listen for clear-search event (emitted when user closes window)
        await listen('clear-search', () => {
            searchInput.value = '';
            showEmptyState();
            searchStats.classList.add('hidden');
        });
    } catch (e) { setStatus(`❌ Init: ${e}`); }
}

// ─── Scope ──────────────────────────────────────────────────
scopeBtns.forEach(btn => {
    btn.addEventListener('click', async () => {
        const scopeName = btn.dataset.scope;
        scopeBtns.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        setStatus(`⏳ Switching to ${scopeName}...`);
        try {
            const r = await invoke('set_scope', { scopeName });
            if (r.ready) { showRoots(r); updateFileCount(r.indexed_count); setStatus(`✅ ${r.message} — re-index to apply`); }
        } catch (e) { setStatus(`❌ ${e}`); }
    });
});

function activateScopeBtn(scopeStr) {
    const name = scopeStr.replace(/"/g, '');
    scopeBtns.forEach(b => {
        b.classList.toggle('active', b.dataset.scope === name);
    });
}

// ─── Add Folder ─────────────────────────────────────────────
btnFolder.addEventListener('click', async () => {
    try {
        const path = await invoke('pick_folder');
        if (path) {
            setStatus(`⏳ Adding ${path}...`);
            const r = await invoke('add_root', { path });
            if (r.ready) { showRoots(r); updateFileCount(r.indexed_count); setStatus(`✅ Added — re-index to scan`); await refreshCustomRoots(); }
        }
    } catch (e) { setStatus(`❌ ${e}`); }
});

// ─── Index (with scan overlay) ──────────────────────────────
btnIndex.addEventListener('click', async () => {
    if (isIndexing) return;
    isIndexing = true;
    btnIndex.disabled = true;
    showScanOverlay();
    try {
        const r = await invoke('index_build');
        hideScanOverlay();
        if (r.success) {
            setStatus(`✅ ${r.indexed.toLocaleString()} new · ${r.skipped.toLocaleString()} unchanged · ${r.took_ms}ms`);
            updateFileCount(r.total_files);
            searchInput.focus();
        } else setStatus(`❌ ${r.message}`);
    } catch (e) { hideScanOverlay(); setStatus(`❌ Index: ${e}`); }
    finally { isIndexing = false; btnIndex.disabled = false; }
});

function showScanOverlay() {
    scanOverlay.classList.remove('hidden');
    scanDetail.textContent = 'Aguarde, isso pode demorar na primeira vez';
    // Animate dots
    let dots = 0;
    scanOverlay._interval = setInterval(() => {
        dots = (dots + 1) % 4;
        scanDetail.textContent = 'Escaneando' + '.'.repeat(dots);
    }, 500);
}
function hideScanOverlay() {
    scanOverlay.classList.add('hidden');
    if (scanOverlay._interval) clearInterval(scanOverlay._interval);
}

// ─── Live Search ────────────────────────────────────────────
searchInput.addEventListener('input', () => {
    clearTimeout(debounceTimer);
    const q = searchInput.value.trim();
    if (q.length < 2) { showEmptyState(); searchStats.classList.add('hidden'); return; }
    debounceTimer = setTimeout(() => doSearch(q), 180);
});

async function doSearch(query) {
    try {
        const r = await invoke('search', { request: { query, limit: 50 } });
        searchStats.textContent = `${r.total_hits} results · ${r.took_ms}ms`;
        searchStats.classList.remove('hidden');
        r.hits.length === 0 ? showNoResults() : renderResults(r.hits);
        addToHistory(query); // Save to history
    } catch (e) { setStatus(`❌ Search: ${e}`); }
}

// ─── Render Results ─────────────────────────────────────────
function renderResults(hits) {
    emptyState.classList.add('hidden');
    noResults.classList.add('hidden');
    resultsList.classList.remove('hidden');
    resultsList.innerHTML = '';
    hits.forEach((hit, i) => {
        const card = document.createElement('div');
        card.className = 'result-card';
        card.style.animationDelay = `${i * 20}ms`;
        card.dataset.path = hit.path;
        card.dataset.name = hit.name;

        const ext = hit.extension || '?';
        const size = hit.size_bytes ? formatSize(hit.size_bytes) : '';
        const date = hit.modified || '';

        card.innerHTML = `
            <div class="result-ext ${getExtClass(ext)}">${esc(ext.substring(0, 4))}</div>
            <div class="result-info">
                <div class="result-name">${esc(hit.name)}</div>
                <div class="result-path">${esc(shortenPath(hit.path))}</div>
                <div class="result-meta">
                    <span class="result-score">${hit.score.toFixed(1)}</span>
                    ${size ? `<span>${size}</span>` : ''}
                    ${date ? `<span>${date}</span>` : ''}
                </div>
            </div>
            <div class="result-actions">
                <button class="action-btn" data-action="folder" title="Abrir local do arquivo">📂</button>
            </div>`;

        // Double-click = open file
        card.addEventListener('dblclick', () => openFile(hit.path));
        // Single click = copy path
        card.addEventListener('click', (e) => {
            if (e.target.closest('.action-btn')) return; // don't copy if clicking action
            copyPath(hit.path);
        });
        // Right-click = context menu
        card.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            showContextMenu(e.clientX, e.clientY, hit.path, hit.name);
        });
        // Action button: open folder
        card.querySelector('[data-action="folder"]').addEventListener('click', (e) => {
            e.stopPropagation();
            openFolder(hit.path);
        });

        resultsList.appendChild(card);
    });
}

// ─── File Actions ───────────────────────────────────────────
async function openFile(path) {
    try {
        await invoke('open_file', { path });
        setStatus(`📄 Opened: ${shortenPath(path)}`);
    } catch (e) { setStatus(`❌ ${e}`); }
}

async function openFolder(path) {
    try {
        await invoke('open_containing_folder', { path });
        setStatus(`📂 Opened folder for: ${shortenPath(path)}`);
    } catch (e) { setStatus(`❌ ${e}`); }
}

async function copyPath(path) {
    try {
        await navigator.clipboard.writeText(path.replace(/^\\\\\?\\/, ''));
        setStatus(`📋 Copied: ${shortenPath(path)}`);
    } catch (e) { setStatus(`📄 ${path}`); }
}

async function copyName(name) {
    try {
        await navigator.clipboard.writeText(name);
        setStatus(`📋 Copied name: ${name}`);
    } catch (e) { setStatus(`📝 ${name}`); }
}

// ─── Context Menu ───────────────────────────────────────────
function showContextMenu(x, y, path, name) {
    contextTarget = { path, name };
    contextMenu.style.left = `${Math.min(x, window.innerWidth - 200)}px`;
    contextMenu.style.top = `${Math.min(y, window.innerHeight - 180)}px`;
    contextMenu.classList.remove('hidden');
}

document.addEventListener('click', () => contextMenu.classList.add('hidden'));
document.addEventListener('contextmenu', (e) => {
    if (!e.target.closest('.result-card')) contextMenu.classList.add('hidden');
});

contextMenu.querySelectorAll('.ctx-item').forEach(item => {
    item.addEventListener('click', () => {
        if (!contextTarget) return;
        const action = item.dataset.action;
        if (action === 'open') openFile(contextTarget.path);
        else if (action === 'folder') openFolder(contextTarget.path);
        else if (action === 'copy-path') copyPath(contextTarget.path);
        else if (action === 'copy-name') copyName(contextTarget.name);
        contextMenu.classList.add('hidden');
    });
});

// ─── Settings Panel ─────────────────────────────────────────
btnSettings.addEventListener('click', async () => {
    settingsPanel.classList.toggle('hidden');
    if (!settingsPanel.classList.contains('hidden')) {
        await refreshScopeRoots();
        await refreshCustomRoots();
    }
});
$('settings-close').addEventListener('click', () => settingsPanel.classList.add('hidden'));

$('btn-add-custom').addEventListener('click', async () => {
    try {
        const path = await invoke('pick_folder');
        if (path) {
            await invoke('add_root', { path });
            setStatus(`✅ Added: ${path}`);
            await refreshCustomRoots();
            await refreshScopeRoots();
        }
    } catch (e) { setStatus(`❌ ${e}`); }
});

// Show scope-detected roots (read-only)
async function refreshScopeRoots() {
    try {
        const status = await invoke('get_status');
        const list = $('scope-roots-list');
        list.innerHTML = '';
        if (!status.roots || status.roots.length === 0) {
            list.innerHTML = '<span class="folder-empty">No folders detected</span>';
            return;
        }
        const customRoots = await invoke('get_custom_roots');
        status.roots.forEach(r => {
            if (customRoots.includes(r)) return; // skip custom ones, shown separately
            const row = document.createElement('div');
            row.className = 'folder-card';
            const folderName = r.split(/[/\\]/).pop();
            const icon = getFolderIcon(folderName);
            row.innerHTML = `
                <span class="folder-icon">${icon}</span>
                <div class="folder-details">
                    <div class="folder-name">${esc(folderName)}</div>
                    <div class="folder-fullpath">${esc(r)}</div>
                </div>
                <span class="folder-badge">auto</span>`;
            list.appendChild(row);
        });
    } catch (e) { /* ignore */ }
}

// Show custom roots (removable)
async function refreshCustomRoots() {
    try {
        const roots = await invoke('get_custom_roots');
        const list = $('custom-roots-list');
        list.innerHTML = '';
        if (roots.length === 0) {
            list.innerHTML = '<span class="folder-empty">No custom folders added</span>';
            return;
        }
        roots.forEach(r => {
            const row = document.createElement('div');
            row.className = 'folder-card folder-card-custom';
            const folderName = r.split(/[/\\]/).pop();
            const icon = getFolderIcon(folderName);
            row.innerHTML = `
                <span class="folder-icon">${icon}</span>
                <div class="folder-details">
                    <div class="folder-name">${esc(folderName)}</div>
                    <div class="folder-fullpath">${esc(r)}</div>
                </div>
                <button class="folder-remove" title="Remove">✕</button>`;
            row.querySelector('.folder-remove').addEventListener('click', async () => {
                await invoke('remove_root', { path: r });
                await refreshCustomRoots();
                setStatus(`Removed: ${r}`);
            });
            list.appendChild(row);
        });
    } catch (e) { /* ignore */ }
}

function getFolderIcon(name) {
    const n = name.toLowerCase();
    if (n.includes('desktop')) return '🖥️';
    if (n.includes('download')) return '⬇️';
    if (n.includes('document') || n.includes('docs')) return '📄';
    if (n.includes('project') || n.includes('projeto')) return '🚀';
    if (n.includes('code') || n.includes('dev') || n.includes('repos')) return '💻';
    if (n.includes('music')) return '🎵';
    if (n.includes('video')) return '🎬';
    if (n.includes('picture') || n.includes('photo')) return '🖼️';
    if (n.includes('work')) return '💼';
    if (n.includes('onedrive')) return '☁️';
    return '📁';
}

// ─── Shortcut Capture ───────────────────────────────────────
let isRecording = false;
const shortcutDisplay = $('shortcut-display');
const btnRecord = $('btn-record-shortcut');

// Load saved shortcut and mode on init
(async () => {
    try {
        const sc = await invoke('get_shortcut');
        shortcutDisplay.textContent = sc;
        
        const mode = await invoke('get_launch_mode');
        if (launchModeSelect) launchModeSelect.value = mode;
    } catch (e) { /* ignore */ }
})();

if (launchModeSelect) {
    launchModeSelect.addEventListener('change', async (e) => {
        try {
            await invoke('set_launch_mode', { mode: e.target.value });
            setStatus(`🚀 Launch mode saved: ${e.target.value}`);
        } catch (err) { setStatus(`❌ ${err}`); }
    });
}

btnRecord.addEventListener('click', () => {
    if (isRecording) {
        stopRecording();
    } else {
        startRecording();
    }
});

function startRecording() {
    isRecording = true;
    btnRecord.textContent = '⏹️ Stop';
    btnRecord.classList.add('recording');
    shortcutDisplay.textContent = 'Press your keys...';
    shortcutDisplay.classList.add('recording-active');
}

function stopRecording() {
    isRecording = false;
    btnRecord.textContent = '🔴 Record';
    btnRecord.classList.remove('recording');
    shortcutDisplay.classList.remove('recording-active');
}

document.addEventListener('keydown', async (e) => {
    // Shortcut recording mode
    if (isRecording) {
        e.preventDefault();
        e.stopPropagation();

        // Build combo string
        const parts = [];
        if (e.ctrlKey) parts.push('Ctrl');
        if (e.altKey) parts.push('Alt');
        if (e.shiftKey) parts.push('Shift');
        if (e.metaKey) parts.push('Super');

        // Add the actual key (skip modifier-only presses)
        const key = e.key;
        if (!['Control', 'Alt', 'Shift', 'Meta'].includes(key)) {
            parts.push(key === ' ' ? 'Space' : key.length === 1 ? key.toUpperCase() : key);

            const combo = parts.join(' + ');
            shortcutDisplay.textContent = combo;
            stopRecording();

            // Auto-save
            try {
                await invoke('set_shortcut', { shortcut: combo });
                setStatus(`⌨️ Shortcut saved: ${combo}`);
            } catch (err) { setStatus(`❌ ${err}`); }
        } else {
            // Show partial combo while holding modifiers
            shortcutDisplay.textContent = parts.join(' + ') + ' + ...';
        }
        return;
    }

    // Normal keyboard shortcuts
    if ((e.ctrlKey||e.metaKey)&&e.key==='f') { e.preventDefault(); searchInput.focus(); searchInput.select(); }
    if (e.key==='Escape') {
        if (!settingsPanel.classList.contains('hidden')) { settingsPanel.classList.add('hidden'); return; }
        searchInput.value = ''; showEmptyState(); searchStats.classList.add('hidden');
    }
});

// ─── Helpers ────────────────────────────────────────────────
function showRoots(r) {
    rootPath.textContent = r.roots.map(p => p.split(/[/\\]/).pop()).join(' · ');
    rootBar.title = r.roots.join('\n');
    rootBar.classList.remove('hidden');
}
function setStatus(t) { statusText.textContent = t; }
function updateFileCount(c) { fileCount.textContent = `${c.toLocaleString()} files`; fileCount.classList.remove('hidden'); }
function showEmptyState() { emptyState.classList.remove('hidden'); resultsList.classList.add('hidden'); noResults.classList.add('hidden'); }
function showNoResults() { emptyState.classList.add('hidden'); resultsList.classList.add('hidden'); noResults.classList.remove('hidden'); }
function shortenPath(p) {
    p = p.replace(/^\\\\\\?\\/,'');
    // Cross-platform home detection: C:\Users\X\ on Windows, /Users/X/ on macOS
    const winMatch = p.match(/^([A-Z]:\\\\Users\\\\[^\\\\]+\\\\)/i);
    const macMatch = p.match(/^(\/Users\/[^/]+\/)/);
    const homePrefix = winMatch ? winMatch[1] : macMatch ? macMatch[1] : null;
    if (homePrefix) {
        p = '~' + (winMatch ? '\\' : '/') + p.substring(homePrefix.length);
    }
    return p;
}
function getExtClass(ext) {
    const m = { rs:'ext-rs', js:'ext-js', jsx:'ext-js', ts:'ext-ts', tsx:'ext-ts', py:'ext-py', md:'ext-md', json:'ext-json', toml:'ext-toml', yaml:'ext-yaml', yml:'ext-yaml', html:'ext-js', css:'ext-ts', txt:'ext-md', pdf:'ext-py' };
    return m[ext] || '';
}
function formatSize(b) { const u=['B','KB','MB','GB']; let i=0,s=b; while(s>=1024&&i<u.length-1){s/=1024;i++;} return `${s.toFixed(i>0?1:0)} ${u[i]}`; }
function esc(s) { const d=document.createElement('div'); d.textContent=s; return d.innerHTML; }

document.addEventListener('DOMContentLoaded', initApp);

