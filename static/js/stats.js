/**
 * HyperCalibrate - Stats Module
 * Handles performance stats and system monitoring
 */

import { showToast, setStatus, formatNumber } from './utils.js';

export class StatsManager {
    constructor(app) {
        this.app = app;
        this.panelVisible = false;
        this.statsInterval = null;
    }

    /**
     * Start stats refresh interval
     */
    start() {
        this.statsInterval = setInterval(() => {
            this.load();
            this.loadSystem();
        }, 1000);
    }

    /**
     * Stop stats refresh
     */
    stop() {
        if (this.statsInterval) {
            clearInterval(this.statsInterval);
            this.statsInterval = null;
        }
    }

    /**
     * Load performance stats
     */
    async load() {
        try {
            const response = await fetch('/api/stats');
            const stats = await response.json();
            this.updateDisplay(stats);
        } catch (error) {
            // Silently fail - non-critical
        }
    }

    /**
     * Load system stats
     */
    async loadSystem() {
        try {
            const response = await fetch('/api/system/stats');
            const stats = await response.json();
            this.updateSystemDisplay(stats);
        } catch (error) {
            // Silently fail - non-critical
        }
    }

    /**
     * Update performance stats display
     */
    updateDisplay(stats) {
        // Footer stats
        const fpsEl = document.getElementById('fps-display');
        const latencyEl = document.getElementById('latency-display');

        if (fpsEl) fpsEl.textContent = stats.fps.toFixed(1) + ' fps';
        if (latencyEl) latencyEl.textContent = stats.timing.avg_processing_ms.toFixed(2) + ' ms';

        // Detailed panel stats
        const setEl = (id, value) => {
            const el = document.getElementById(id);
            if (el) el.textContent = value;
        };

        setEl('stat-frame-wait', stats.timing.avg_frame_wait_ms.toFixed(2) + ' ms');
        setEl('stat-decode', stats.timing.avg_decode_ms.toFixed(2) + ' ms');
        setEl('stat-transform', stats.timing.avg_transform_ms.toFixed(2) + ' ms');
        setEl('stat-output', stats.timing.avg_output_ms.toFixed(2) + ' ms');

        if (stats.preview_active && stats.preview_frames_encoded > 0) {
            setEl('stat-preview', stats.timing.avg_preview_encode_ms.toFixed(2) + ' ms');
        } else {
            setEl('stat-preview', 'N/A');
        }

        setEl('stat-processing', stats.timing.avg_processing_ms.toFixed(2) + ' ms');
        setEl('stat-pipeline', stats.timing.avg_pipeline_ms.toFixed(2) + ' ms');
        setEl('stat-frames', formatNumber(stats.frames_processed));

        // Preview status
        const previewStatusEl = document.getElementById('preview-status');
        if (previewStatusEl) {
            if (stats.preview_active) {
                previewStatusEl.textContent = 'Encoding: Active';
                previewStatusEl.className = 'preview-status active';
            } else {
                previewStatusEl.textContent = 'Encoding: Off';
                previewStatusEl.className = 'preview-status inactive';
            }
        }
    }

    /**
     * Update system stats display
     */
    updateSystemDisplay(stats) {
        // CPU Temperature
        const cpuTempEl = document.getElementById('stat-cpu-temp');
        const cpuTempMini = document.getElementById('cpu-temp-mini');
        if (stats.cpu_temp_c !== null) {
            const temp = stats.cpu_temp_c;
            if (cpuTempEl) {
                cpuTempEl.textContent = temp.toFixed(1) + ' °C';
                if (temp >= 80) {
                    cpuTempEl.className = 'stat-value temp-critical';
                } else if (temp >= 70) {
                    cpuTempEl.className = 'stat-value temp-warning';
                } else {
                    cpuTempEl.className = 'stat-value temp-ok';
                }
            }
            if (cpuTempMini) {
                cpuTempMini.textContent = '🌡️ ' + temp.toFixed(0) + '°C';
                if (temp >= 80) {
                    cpuTempMini.className = 'mini-stat temp-critical';
                } else if (temp >= 70) {
                    cpuTempMini.className = 'mini-stat temp-warning';
                } else {
                    cpuTempMini.className = 'mini-stat temp-ok';
                }
            }
        } else {
            if (cpuTempEl) {
                cpuTempEl.textContent = 'N/A';
                cpuTempEl.className = 'stat-value';
            }
            if (cpuTempMini) {
                cpuTempMini.textContent = '🌡️ --°C';
                cpuTempMini.className = 'mini-stat';
            }
        }

        // Memory usage
        const memEl = document.getElementById('stat-memory');
        const memMini = document.getElementById('memory-mini');
        if (memEl) {
            memEl.textContent = stats.mem_used_percent.toFixed(0) + '%';
            if (stats.mem_used_percent >= 90) {
                memEl.className = 'stat-value temp-critical';
            } else if (stats.mem_used_percent >= 75) {
                memEl.className = 'stat-value temp-warning';
            } else {
                memEl.className = 'stat-value';
            }
        }
        if (memMini) {
            memMini.textContent = '💾 ' + stats.mem_used_percent.toFixed(0) + '%';
            if (stats.mem_used_percent >= 90) {
                memMini.className = 'mini-stat temp-critical';
            } else if (stats.mem_used_percent >= 75) {
                memMini.className = 'mini-stat temp-warning';
            } else {
                memMini.className = 'mini-stat';
            }
        }

        // Load average
        const loadEl = document.getElementById('stat-load');
        const loadMini = document.getElementById('load-mini');
        if (stats.load_avg_1m !== null) {
            if (loadEl) {
                loadEl.textContent = stats.load_avg_1m.toFixed(2);
                if (stats.load_avg_1m >= 4) {
                    loadEl.className = 'stat-value temp-critical';
                } else if (stats.load_avg_1m >= 2) {
                    loadEl.className = 'stat-value temp-warning';
                } else {
                    loadEl.className = 'stat-value';
                }
            }
            if (loadMini) {
                loadMini.textContent = '📈 ' + stats.load_avg_1m.toFixed(1);
                if (stats.load_avg_1m >= 4) {
                    loadMini.className = 'mini-stat temp-critical';
                } else if (stats.load_avg_1m >= 2) {
                    loadMini.className = 'mini-stat temp-warning';
                } else {
                    loadMini.className = 'mini-stat';
                }
            }
        } else {
            if (loadEl) loadEl.textContent = 'N/A';
            if (loadMini) loadMini.textContent = '📈 --';
        }

        // Throttle status
        const throttleEl = document.getElementById('stat-throttle');
        if (throttleEl && stats.throttled) {
            const t = stats.throttled;
            let status = [];
            let isCritical = false;
            let isWarning = false;

            if (t.under_voltage) { status.push('UV!'); isCritical = true; }
            if (t.throttled) { status.push('THR!'); isCritical = true; }
            if (t.freq_capped) { status.push('CAP'); isWarning = true; }
            if (t.soft_temp_limit) { status.push('HOT'); isWarning = true; }

            if (status.length === 0) {
                if (t.under_voltage_occurred || t.throttled_occurred) {
                    status.push('PREV');
                    isWarning = true;
                }
            }

            if (status.length > 0) {
                throttleEl.textContent = status.join(' ');
                throttleEl.className = 'stat-value ' + (isCritical ? 'temp-critical' : 'temp-warning');
                throttleEl.title = this.formatThrottleTooltip(t);
            } else {
                throttleEl.textContent = 'OK';
                throttleEl.className = 'stat-value temp-ok';
                throttleEl.title = 'No throttling detected';
            }
        } else if (throttleEl) {
            throttleEl.textContent = '--';
            throttleEl.className = 'stat-value';
            throttleEl.title = 'Throttle status not available';
        }
    }

    /**
     * Format throttle tooltip
     */
    formatThrottleTooltip(t) {
        let lines = [];
        if (t.under_voltage) lines.push('⚠️ Under-voltage NOW');
        if (t.throttled) lines.push('⚠️ Throttled NOW');
        if (t.freq_capped) lines.push('⚠️ Frequency capped');
        if (t.soft_temp_limit) lines.push('⚠️ Soft temp limit');
        if (t.under_voltage_occurred) lines.push('📋 Under-voltage occurred');
        if (t.throttled_occurred) lines.push('📋 Throttling occurred');
        if (t.freq_capped_occurred) lines.push('📋 Freq capping occurred');
        if (t.soft_temp_limit_occurred) lines.push('📋 Temp limit occurred');
        return lines.length > 0 ? lines.join('\n') : 'System healthy';
    }

    /**
     * Toggle stats panel visibility
     */
    toggle() {
        this.panelVisible = !this.panelVisible;
        const panel = document.getElementById('stats-panel');
        if (panel) {
            if (this.panelVisible) {
                panel.classList.remove('hidden');
            } else {
                panel.classList.add('hidden');
            }
        }
        return this.panelVisible;
    }

    /**
     * Reset stats on server
     */
    async reset() {
        try {
            await fetch('/api/stats/reset', { method: 'POST' });
            showToast('Stats reset', 'success');
        } catch (error) {
            console.error('Failed to reset stats:', error);
            showToast('Failed to reset stats', 'error');
        }
    }

    /**
     * Restart the service
     */
    async restart() {
        if (!confirm('Restart HyperCalibrate service?\n\nThe page will reload automatically when the service is back up.')) {
            return;
        }

        try {
            showToast('Restarting service...', 'info');
            setStatus('Restarting...', 'warning');

            await fetch('/api/system/restart', { method: 'POST' });
            setTimeout(() => this.waitForRestart(), 2000);
        } catch (error) {
            console.error('Failed to restart service:', error);
            showToast('Failed to send restart request', 'error');
        }
    }

    /**
     * Wait for service to come back after restart
     */
    async waitForRestart() {
        const maxAttempts = 30;
        let attempts = 0;

        const checkService = async () => {
            attempts++;
            try {
                const response = await fetch('/api/info', {
                    method: 'GET',
                    cache: 'no-store'
                });
                if (response.ok) {
                    showToast('Service restarted successfully', 'success');
                    window.location.reload();
                    return;
                }
            } catch (e) {
                // Still down
            }

            if (attempts < maxAttempts) {
                setStatus(`Waiting for restart... (${attempts}s)`, 'warning');
                setTimeout(checkService, 1000);
            } else {
                setStatus('Restart timeout', 'error');
                showToast('Service did not come back up. Check the Pi.', 'error');
            }
        };

        checkService();
    }
}
