/**
 * HyperCalibrate - Video Settings Module
 * Handles video resolution and framerate configuration
 */

import { showToast, setStatus } from './utils.js';

export class VideoManager {
    constructor(app) {
        this.app = app;
        this.capabilities = null;
        this.pendingSettings = null;
        this.width = 640;
        this.height = 480;
        this.fps = 30;
        this.selectedResolution = null;
        this.selectedFps = null;
        this.devices = [];
        this.currentDevice = '';
        this.selectedDevice = null;
        this.pendingDevice = false;
        this.currentFormat = 'mjpeg';
        this.selectedFormat = null;
        this.pendingFormat = false;
    }

    /**
     * Load video settings and capabilities
     */
    async load() {
        const loadingEl = document.getElementById('video-settings-loading');
        const contentEl = document.getElementById('video-settings-content');
        const unavailableEl = document.getElementById('video-settings-unavailable');

        loadingEl?.classList.remove('hidden');
        contentEl?.classList.add('hidden');
        unavailableEl?.classList.add('hidden');

        try {
            const response = await fetch('/api/video/capabilities');
            if (!response.ok) {
                throw new Error('Failed to fetch video capabilities');
            }

            const data = await response.json();
            this.capabilities = data.capabilities;

            this.width = data.current.width;
            this.height = data.current.height;
            this.fps = data.current.fps;
            this.selectedResolution = `${data.current.width}x${data.current.height}`;
            this.selectedFps = data.current.fps;

            loadingEl?.classList.add('hidden');

            if (!this.capabilities || this.capabilities.resolutions.length === 0) {
                unavailableEl?.classList.remove('hidden');
                return;
            }

            this.render();
            contentEl?.classList.remove('hidden');

            await this.loadDevices();
            await this.loadFormat();
            await this.checkPending();
        } catch (error) {
            console.error('Failed to load video settings:', error);
            loadingEl?.classList.add('hidden');
            unavailableEl?.classList.remove('hidden');
        }
    }

    /**
     * Load available video devices
     */
    async loadDevices() {
        try {
            const response = await fetch('/api/video/devices');
            if (!response.ok) return;

            const data = await response.json();
            this.devices = data.devices || [];
            this.currentDevice = data.current || '';
            this.selectedDevice = this.currentDevice;

            this.renderDevices();
        } catch (error) {
            console.error('Failed to load video devices:', error);
        }
    }

    /**
     * Render device selector
     */
    renderDevices() {
        const deviceSelect = document.getElementById('device-select');
        const currentDeviceEl = document.getElementById('current-device');

        if (!deviceSelect) return;

        // Show short name for current device
        const currentDev = this.devices.find(d => d.path === this.currentDevice);
        if (currentDeviceEl) {
            currentDeviceEl.textContent = currentDev ? currentDev.name.substring(0, 20) : this.currentDevice.split('/').pop();
        }

        deviceSelect.innerHTML = '';
        for (const device of this.devices) {
            const option = document.createElement('option');
            option.value = device.path;
            option.textContent = `${device.path.split('/').pop()}: ${device.name}`;
            if (device.path === this.currentDevice) {
                option.selected = true;
            }
            deviceSelect.appendChild(option);
        }

        // Add event listener
        deviceSelect.addEventListener('change', (e) => this.onDeviceChange(e.target.value));
    }

    /**
     * Handle device change
     */
    onDeviceChange(value) {
        this.selectedDevice = value;
        this.pendingDevice = value !== this.currentDevice;
        this.updatePendingState();
    }

    /**
     * Load current video format
     */
    async loadFormat() {
        try {
            const response = await fetch('/api/video/format');
            if (!response.ok) return;

            const data = await response.json();
            this.currentFormat = data.format || 'mjpeg';
            this.selectedFormat = this.currentFormat;

            this.renderFormat();
        } catch (error) {
            console.error('Failed to load video format:', error);
        }
    }

    /**
     * Render format selector
     */
    renderFormat() {
        const formatSelect = document.getElementById('format-select');
        const currentFormatEl = document.getElementById('current-format');

        if (!formatSelect) return;

        // Show current format
        if (currentFormatEl) {
            currentFormatEl.textContent = this.currentFormat.toUpperCase();
        }

        formatSelect.value = this.currentFormat;

        // Add event listener (only once)
        if (!formatSelect.dataset.listenerAdded) {
            formatSelect.addEventListener('change', (e) => this.onFormatChange(e.target.value));
            formatSelect.dataset.listenerAdded = 'true';
        }
    }

    /**
     * Handle format change
     */
    onFormatChange(value) {
        this.selectedFormat = value;
        this.pendingFormat = value !== this.currentFormat;
        this.updatePendingState();
    }

    /**
     * Render video settings UI
     */
    render() {
        const resolutionSelect = document.getElementById('resolution-select');
        const fpsSelect = document.getElementById('fps-select');
        const currentResEl = document.getElementById('current-resolution');
        const currentFpsEl = document.getElementById('current-fps');

        currentResEl.textContent = `${this.width}×${this.height}`;
        currentFpsEl.textContent = `${this.fps} fps`;

        // Populate resolution dropdown
        resolutionSelect.innerHTML = '';
        const resolutions = [...this.capabilities.resolutions];
        resolutions.sort((a, b) => (b.width * b.height) - (a.width * a.height));

        for (const res of resolutions) {
            const option = document.createElement('option');
            option.value = `${res.width}x${res.height}`;
            option.textContent = `${res.width} × ${res.height}`;
            if (res.width === this.width && res.height === this.height) {
                option.selected = true;
            }
            resolutionSelect.appendChild(option);
        }

        this.updateFpsOptions();
    }

    /**
     * Update FPS options for selected resolution
     */
    updateFpsOptions() {
        const fpsSelect = document.getElementById('fps-select');
        fpsSelect.innerHTML = '';

        const [width, height] = this.selectedResolution.split('x').map(Number);
        const resInfo = this.capabilities.resolutions.find(
            r => r.width === width && r.height === height
        );

        if (!resInfo || !resInfo.framerates || resInfo.framerates.length === 0) {
            const defaultFps = [30, 25, 20, 15, 10];
            for (const fps of defaultFps) {
                const option = document.createElement('option');
                option.value = fps;
                option.textContent = `${fps} fps`;
                if (fps === this.selectedFps || fps === this.fps) {
                    option.selected = true;
                }
                fpsSelect.appendChild(option);
            }
            return;
        }

        const framerates = [...resInfo.framerates].sort((a, b) => b.fps - a.fps);

        for (const fr of framerates) {
            const option = document.createElement('option');
            option.value = Math.round(fr.fps);
            option.textContent = `${fr.fps.toFixed(fr.fps % 1 === 0 ? 0 : 2)} fps`;
            if (Math.round(fr.fps) === this.selectedFps || Math.round(fr.fps) === this.fps) {
                option.selected = true;
            }
            fpsSelect.appendChild(option);
        }

        if (!fpsSelect.value) {
            fpsSelect.selectedIndex = 0;
            this.selectedFps = parseInt(fpsSelect.value);
        }
    }

    /**
     * Handle resolution change
     */
    onResolutionChange(value) {
        this.selectedResolution = value;
        this.updateFpsOptions();
        this.updatePendingState();
    }

    /**
     * Handle FPS change
     */
    onFpsChange(value) {
        this.selectedFps = value;
        this.updatePendingState();
    }

    /**
     * Update pending changes state
     */
    updatePendingState() {
        const [selectedWidth, selectedHeight] = this.selectedResolution.split('x').map(Number);
        const hasChanges =
            selectedWidth !== this.width ||
            selectedHeight !== this.height ||
            this.selectedFps !== this.fps ||
            this.pendingDevice ||
            this.pendingFormat;

        const noticeEl = document.getElementById('pending-settings-notice');
        const applyBtn = document.getElementById('apply-video-settings-btn');

        if (hasChanges) {
            noticeEl?.classList.remove('hidden');
            applyBtn?.classList.remove('hidden');
        } else {
            noticeEl?.classList.add('hidden');
            applyBtn?.classList.add('hidden');
        }
    }

    /**
     * Check for pending settings from server
     */
    async checkPending() {
        try {
            const response = await fetch('/api/video/settings');
            const data = await response.json();

            if (data.pending) {
                this.pendingSettings = data.pending;

                if (data.pending.width && data.pending.height) {
                    this.selectedResolution = `${data.pending.width}x${data.pending.height}`;
                    document.getElementById('resolution-select').value = this.selectedResolution;
                }
                if (data.pending.fps) {
                    this.selectedFps = data.pending.fps;
                    this.updateFpsOptions();
                }

                this.updatePendingState();
            }
        } catch (error) {
            console.error('Failed to check pending settings:', error);
        }
    }

    /**
     * Apply video settings (requires restart)
     */
    async apply() {
        const [width, height] = this.selectedResolution.split('x').map(Number);
        const fps = this.selectedFps;

        const applyBtn = document.getElementById('apply-video-settings-btn');
        applyBtn.disabled = true;
        applyBtn.textContent = 'Applying...';

        try {
            let needsRestart = false;

            // Save device change if pending
            if (this.pendingDevice && this.selectedDevice !== this.currentDevice) {
                const deviceResponse = await fetch('/api/video/device', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ device: this.selectedDevice })
                });

                const deviceResult = await deviceResponse.json();
                if (!deviceResponse.ok) {
                    throw new Error(deviceResult.message || 'Failed to save device');
                }
                if (deviceResult.restart_required) {
                    needsRestart = true;
                }
            }

            // Save format change if pending
            if (this.pendingFormat && this.selectedFormat !== this.currentFormat) {
                const formatResponse = await fetch('/api/video/format', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ format: this.selectedFormat })
                });

                const formatResult = await formatResponse.json();
                if (!formatResponse.ok) {
                    throw new Error(formatResult.message || 'Failed to save format');
                }
                if (formatResult.restart_required) {
                    needsRestart = true;
                }
            }

            // Save resolution/fps changes
            const [selectedWidth, selectedHeight] = this.selectedResolution.split('x').map(Number);
            const hasVideoChanges = selectedWidth !== this.width ||
                                    selectedHeight !== this.height ||
                                    this.selectedFps !== this.fps;

            if (hasVideoChanges) {
                const saveResponse = await fetch('/api/video/settings', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ width, height, fps })
                });

                const saveResult = await saveResponse.json();

                if (!saveResponse.ok) {
                    throw new Error(saveResult.message || 'Failed to save settings');
                }
                if (saveResult.restart_required) {
                    needsRestart = true;
                }
            }

            if (needsRestart) {
                showToast('Settings saved. Restarting service...', 'success');

                const restartResponse = await fetch('/api/system/restart', { method: 'POST' });
                const restartResult = await restartResponse.json();

                if (restartResult.success) {
                    setStatus('Restarting...', 'warning');
                    applyBtn.textContent = 'Restarting...';
                    this.waitForReconnect();
                } else {
                    throw new Error(restartResult.message || 'Failed to restart');
                }
            } else {
                showToast('No changes to apply.', 'info');
                applyBtn.disabled = false;
                applyBtn.textContent = 'Apply & Restart';
                this.updatePendingState();
            }
        } catch (error) {
            console.error('Failed to apply video settings:', error);
            showToast('Failed to apply settings: ' + error.message, 'error');
            applyBtn.disabled = false;
            applyBtn.textContent = 'Apply & Restart';
        }
    }

    /**
     * Wait for service to reconnect after restart
     */
    async waitForReconnect() {
        const maxAttempts = 30;
        const delayMs = 1000;
        let attempts = 0;

        setStatus('Reconnecting...', 'warning');

        const checkConnection = async () => {
            attempts++;
            try {
                const response = await fetch('/api/info', {
                    cache: 'no-store',
                    signal: AbortSignal.timeout(2000)
                });
                if (response.ok) {
                    showToast('Service restarted successfully!', 'success');
                    setTimeout(() => window.location.reload(), 1000);
                    return;
                }
            } catch (error) {
                // Still down
            }

            if (attempts < maxAttempts) {
                setTimeout(checkConnection, delayMs);
            } else {
                setStatus('Connection lost', 'error');
                showToast('Service may still be restarting. Try refreshing the page.', 'warning');
            }
        };

        setTimeout(checkConnection, 2000);
    }
}
