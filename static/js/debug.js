/**
 * HyperCalibrate - Debug Module
 * Debug logging overlay and server-side log sync
 */

export class DebugManager {
    constructor() {
        this.enabled = false;
        this.logElement = null;
        this.pendingLogs = [];
        this.flushTimer = null;
    }

    /**
     * Initialize debug manager
     */
    init() {
        this.logElement = document.getElementById('debug-log');
        
        document.getElementById('toggle-debug')?.addEventListener('click', () => {
            this.toggle();
        });
        
        document.getElementById('debug-clear')?.addEventListener('click', () => {
            this.clearLog();
        });
        
        document.getElementById('debug-select-all')?.addEventListener('click', () => {
            this.selectAll();
        });
    }

    /**
     * Toggle debug mode on/off
     */
    toggle() {
        this.enabled = !this.enabled;
        const overlay = document.getElementById('debug-overlay');
        const btn = document.getElementById('toggle-debug');

        if (this.enabled) {
            overlay?.classList.remove('hidden');
            btn?.classList.add('active');
            
            // Log initial state info
            this.log('info', 'Debug mode enabled');
            this.log('info', `isMobile: ${'ontouchstart' in window || navigator.maxTouchPoints > 0}`);
        } else {
            overlay?.classList.add('hidden');
            btn?.classList.remove('active');
        }
    }

    /**
     * Log a debug message
     * @param {string} type - Log type ('event', 'coords', 'hit', 'miss', 'info', 'longpress')
     * @param {string} message - Message to log
     */
    log(type, message) {
        if (!this.enabled) return;

        const time = new Date().toLocaleTimeString('en-US', {
            hour12: false,
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            fractionalSecondDigits: 3
        });

        // Add to pending logs for server
        this.pendingLogs.push({ time, type, message });

        // Batch send to server every 100ms
        if (!this.flushTimer) {
            this.flushTimer = setTimeout(() => this.flushLogs(), 100);
        }

        // Also show in UI if element exists
        if (this.logElement) {
            const entry = document.createElement('div');
            entry.className = 'debug-entry';

            let typeClass = 'debug-info';
            if (type === 'event') typeClass = 'debug-event';
            else if (type === 'coords') typeClass = 'debug-coords';
            else if (type === 'hit') typeClass = 'debug-hit';
            else if (type === 'miss') typeClass = 'debug-miss';
            else if (type === 'longpress') typeClass = 'debug-event';

            entry.innerHTML = `<span class="debug-time">${time}</span> <span class="${typeClass}">${message}</span>`;

            this.logElement.appendChild(entry);
            this.logElement.scrollTop = this.logElement.scrollHeight;

            // Keep only last 50 entries in UI
            while (this.logElement.children.length > 50) {
                this.logElement.removeChild(this.logElement.firstChild);
            }
        }
    }

    /**
     * Flush pending logs to server
     */
    flushLogs() {
        this.flushTimer = null;

        if (this.pendingLogs.length === 0) return;

        const entries = this.pendingLogs;
        this.pendingLogs = [];

        // Send to server (fire and forget)
        fetch('/api/debug/log', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ entries })
        }).catch(() => {
            // Ignore errors - debug logging shouldn't break the app
        });
    }

    /**
     * Clear the debug log (UI and server)
     */
    clearLog() {
        if (this.logElement) {
            this.logElement.innerHTML = '';
        }
        // Also clear server-side log
        fetch('/api/debug/clear', { method: 'POST' }).catch(() => {});
    }

    /**
     * Select all text in the debug log for easy copying
     */
    selectAll() {
        if (this.logElement) {
            const selection = window.getSelection();
            const range = document.createRange();
            range.selectNodeContents(this.logElement);
            selection.removeAllRanges();
            selection.addRange(range);
        }
    }
}

// Export singleton instance
export const debug = new DebugManager();
