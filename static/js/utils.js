/**
 * HyperCalibrate - Utility Functions
 * Shared helpers for UI, formatting, and common operations
 */

/**
 * Show a toast notification
 * @param {string} message - Message to display
 * @param {string} type - 'success', 'error', or 'warning'
 */
export function showToast(message, type = 'success') {
    const toast = document.getElementById('toast');
    if (!toast) return;
    
    toast.textContent = message;
    toast.className = 'toast ' + type + ' show';

    setTimeout(() => {
        toast.classList.remove('show');
    }, 3000);
}

/**
 * Set the connection status indicator
 * @param {HTMLElement} statusElement - The status element
 * @param {string} text - Status text
 * @param {string} className - CSS class ('connected', 'error', 'warning')
 */
export function setStatus(statusElement, text, className) {
    if (!statusElement) return;
    statusElement.textContent = text;
    statusElement.className = 'status ' + className;
}

/**
 * Format a large number with K/M suffix
 * @param {number} num - Number to format
 * @returns {string} Formatted string
 */
export function formatNumber(num) {
    if (num >= 1000000) {
        return (num / 1000000).toFixed(1) + 'M';
    } else if (num >= 1000) {
        return (num / 1000).toFixed(1) + 'K';
    }
    return num.toString();
}

/**
 * Format a control name from snake_case to Title Case
 * @param {string} name - Control name
 * @returns {string} Formatted name
 */
export function formatControlName(name) {
    return name
        .replace(/_/g, ' ')
        .replace(/\b\w/g, c => c.toUpperCase());
}

/**
 * Normalize a control name (lowercase, replace spaces/commas with underscores)
 * @param {string} name - Control name
 * @returns {string} Normalized name
 */
export function normalizeControlName(name) {
    return name.toLowerCase().replace(/[,\s]+/g, '_');
}

/**
 * Detect if running on a mobile/touch device
 * @returns {boolean}
 */
export function isMobileDevice() {
    return 'ontouchstart' in window || navigator.maxTouchPoints > 0;
}

/**
 * Create a throttled version of a function
 * @param {Function} func - Function to throttle
 * @param {number} limit - Minimum interval between calls (ms)
 * @returns {Function} Throttled function
 */
export function throttle(func, limit) {
    let lastCall = 0;
    return function(...args) {
        const now = Date.now();
        if (now - lastCall >= limit) {
            lastCall = now;
            return func.apply(this, args);
        }
    };
}
