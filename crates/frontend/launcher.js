const { invoke } = window.__TAURI__.core;
const { appWindow } = window.__TAURI__.window;

const input = document.getElementById('launcher-input');
const resultsList = document.getElementById('launcher-results');

let debounceTimer = null;
let currentHits = [];
let selectedIndex = -1;

// Config initial state
input.focus();

// Auto-hide when losing focus (like Spotlight/Alfred)
window.addEventListener('blur', async () => {
    // Esconde a janela se o utilizador clicar fora dela
    try {
        await invoke('hide_launcher_window');
    } catch(err) {
        console.warn('Fallback: could not notify backend', err);
    }
});

// Intercept arrow keys for navigation, and Enter for execution
input.addEventListener('keydown', async (e) => {
    if (e.key === 'ArrowDown') {
        e.preventDefault();
        moveSelection(1);
    } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        moveSelection(-1);
    } else if (e.key === 'Enter') {
        e.preventDefault();
        await executeSelected();
    } else if (e.key === 'Escape') {
        e.preventDefault();
        // Hide window on esc
        await invoke('hide_launcher_window');
    }
});

input.addEventListener('input', (e) => {
    clearTimeout(debounceTimer);
    const query = e.target.value.trim();
    
    if (query.length < 2) {
        currentHits = [];
        renderResults();
        return;
    }

    debounceTimer = setTimeout(() => {
        performSearch(query);
    }, 120); // Fast debounce for a snappy launcher
});

async function performSearch(query) {
    try {
        // Query Tauri backend, fetch only top 10 results for speed
        const resp = await invoke('search', { request: { query, limit: 10 } });
        currentHits = resp.hits || [];
        // Auto-select the first item
        selectedIndex = currentHits.length > 0 ? 0 : -1;
        renderResults();
    } catch (err) {
        console.error("Launcher search error:", err);
        currentHits = [];
        selectedIndex = -1;
        renderResults();
    }
}

function renderResults() {
    resultsList.innerHTML = '';
    
    if (currentHits.length === 0) {
        resultsList.classList.add('hidden');
        return;
    }
    
    resultsList.classList.remove('hidden');
    
    currentHits.forEach((hit, index) => {
        const item = document.createElement('div');
        item.className = 'result-item';
        if (index === selectedIndex) {
            item.classList.add('selected');
        }
        
        // Quick icon maps
        const isDir = !hit.extension;
        const icon = isDir ? '📁' : '📄';
        
        let pathStr = hit.path;
        // Basic shortening
        pathStr = pathStr.replace(/^\\\\\?\\/, '');
        pathStr = shortenPath(pathStr);
        
        item.innerHTML = `
            <div class="item-icon">${icon}</div>
            <div class="item-details">
                <div class="item-name" title="${hit.name}">${highlight(hit.name)}</div>
                <div class="item-path" title="${pathStr}">${pathStr}</div>
            </div>
        `;
        
        // Click to execute
        item.addEventListener('click', async () => {
            selectedIndex = index;
            await executeSelected();
        });
        
        resultsList.appendChild(item);
    });
    
    // Auto rescale window via CSS (Tauri handles dynamic height if set, otherwise it scrolls)
}

function moveSelection(direction) {
    if (currentHits.length === 0) return;
    
    selectedIndex += direction;
    if (selectedIndex >= currentHits.length) {
        selectedIndex = 0; // wrap around
    } else if (selectedIndex < 0) {
        selectedIndex = currentHits.length - 1; // wrap around
    }
    
    // Update visuals
    const items = document.querySelectorAll('.result-item');
    items.forEach(el => el.classList.remove('selected'));
    
    const selectedEl = items[selectedIndex];
    if (selectedEl) {
        selectedEl.classList.add('selected');
        selectedEl.scrollIntoView({ block: 'nearest' });
    }
}

async function executeSelected() {
    if (selectedIndex < 0 || selectedIndex >= currentHits.length) return;
    
    const hit = currentHits[selectedIndex];
    if (!hit) return;
    
    try {
        await invoke('open_file', { path: hit.path });
        // Close the launcher upon successful execution
        await invoke('hide_launcher_window');
        // Clear input for next time
        input.value = '';
        currentHits = [];
        renderResults();
    } catch(err) {
        console.error("Failed to execute file:", err);
    }
}

// Helper to highlight search matches rudimentarily
function highlight(text) {
    const q = input.value.trim();
    if (!q) return text;
    // VERY simple highlighting (case-insensitive)
    const regex = new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
    return text.replace(regex, `<span style="color: var(--accent); font-weight: bold;">$1</span>`);
}

function shortenPath(p) {
    const winMatch = p.match(/^([A-Z]:\\[Uu]sers\\[^\\]+\\)/i);
    const macMatch = p.match(/^(\/Users\/[^\/]+\/)/);
    const homePrefix = winMatch ? winMatch[1] : macMatch ? macMatch[1] : null;
    if (homePrefix) {
        p = '~' + (winMatch ? '\\' : '/') + p.substring(homePrefix.length);
    }
    return p;
}
