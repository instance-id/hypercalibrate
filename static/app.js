/**
 * HyperCalibrate - Calibration UI Application
 *
 * Points:
 * - Corners (IDs 0-3): TL, TR, BR, BL - always present
 * - Edge points (IDs 100+): dynamic, can be added/removed
 *
 * Edges connect corners:
 * - Edge 0: TL (0) -> TR (1) - Top
 * - Edge 1: TR (1) -> BR (2) - Right
 * - Edge 2: BR (2) -> BL (3) - Bottom
 * - Edge 3: BL (3) -> TL (0) - Left
 */

class HyperCalibrate {
    constructor() {
        this.corners = [];
        this.edgePoints = [];
        this.width = 640;
        this.height = 480;
        this.fps = 30;
        this.calibrationEnabled = true;
        this.showCorrected = false;
        this.draggingPoint = null;
        this.previewElement = null;
        this.overlayElement = null;
        this.refreshInterval = null;
        this.statsInterval = null;
        this.cameraControls = [];
        this.cameraPanelVisible = false;
        this.statsPanelVisible = false;
        this.livePreviewEnabled = false;  // Start with snapshot mode

        // Video settings
        this.videoCapabilities = null;
        this.pendingVideoSettings = null;
        this.selectedResolution = null;
        this.selectedFps = null;

        this.edgeCorners = [
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 0],
        ];

        this.init();
    }

    async init() {
        this.previewElement = document.getElementById('preview');
        this.overlayElement = document.getElementById('calibration-overlay');
        this.previewWrapper = document.getElementById('preview-wrapper');
        this.previewContainer = document.querySelector('.preview-container');
        this.statusElement = document.getElementById('status');
        this.cameraPanelElement = document.getElementById('camera-panel');

        this.setupEventListeners();

        await this.loadInfo();
        await this.loadCalibration();
        await this.loadVideoSettings();
        await this.loadCameraControls();

        // Capture initial snapshot (don't start live preview by default)
        await this.captureSnapshot();

        this.startStatsRefresh();
        this.setStatus('Connected', 'connected');

        // Handle page visibility changes
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                this.stopLivePreview();
            } else if (this.livePreviewEnabled) {
                this.startLivePreview();
            }
        });

        // Cleanup on page unload
        window.addEventListener('beforeunload', () => {
            this.stopLivePreview();
        });
    }

    setupEventListeners() {
        document.getElementById('calibration-enabled').addEventListener('change', (e) => {
            this.toggleCalibration(e.target.checked);
        });

        document.getElementById('show-corrected').addEventListener('change', (e) => {
            this.showCorrected = e.target.checked;
            // If not in live mode, capture a new snapshot with new setting
            if (!this.livePreviewEnabled) {
                this.captureSnapshot();
            }
        });

        // Preview controls
        document.getElementById('capture-snapshot-btn').addEventListener('click', () => {
            this.captureSnapshot();
        });

        document.getElementById('live-preview-enabled').addEventListener('change', (e) => {
            this.toggleLivePreview(e.target.checked);
        });

        document.getElementById('reset-btn').addEventListener('click', () => {
            this.resetCalibration();
        });

        document.getElementById('save-btn').addEventListener('click', () => {
            this.saveCalibration();
        });

        // Camera panel controls
        document.getElementById('toggle-camera-panel').addEventListener('click', () => {
            this.toggleCameraPanel();
        });

        document.getElementById('close-camera-panel').addEventListener('click', () => {
            this.toggleCameraPanel(false);
        });

        document.getElementById('reset-camera-btn').addEventListener('click', () => {
            this.resetCameraControls();
        });

        document.getElementById('refresh-camera-btn').addEventListener('click', () => {
            this.refreshCameraControls();
        });

        // Video settings controls
        document.getElementById('resolution-select').addEventListener('change', (e) => {
            this.onResolutionChange(e.target.value);
        });

        document.getElementById('fps-select').addEventListener('change', (e) => {
            this.onFpsChange(parseInt(e.target.value));
        });

        document.getElementById('apply-video-settings-btn').addEventListener('click', () => {
            this.applyVideoSettings();
        });

        // Stats panel controls
        document.getElementById('toggle-stats').addEventListener('click', () => {
            this.toggleStatsPanel();
        });

        document.getElementById('reset-stats-btn').addEventListener('click', () => {
            this.resetStats();
        });

        document.getElementById('restart-service-btn').addEventListener('click', () => {
            this.restartService();
        });

        this.previewElement.addEventListener('load', () => {
            this.syncOverlaySize();
        });

        window.addEventListener('resize', () => {
            this.syncOverlaySize();
        });

        // Use ResizeObserver for more robust overlay syncing
        // This catches layout changes from panel toggles, etc.
        if (typeof ResizeObserver !== 'undefined') {
            const resizeObserver = new ResizeObserver(() => {
                this.syncOverlaySize();
            });
            resizeObserver.observe(this.previewWrapper);
            // Also observe the container for height changes
            if (this.previewContainer) {
                resizeObserver.observe(this.previewContainer);
            }
        }

        this.overlayElement.addEventListener('mousedown', (e) => this.onPointerDown(e));
        this.overlayElement.addEventListener('mousemove', (e) => this.onPointerMove(e));
        this.overlayElement.addEventListener('mouseup', (e) => this.onPointerUp(e));
        this.overlayElement.addEventListener('mouseleave', (e) => this.onPointerUp(e));

        this.overlayElement.addEventListener('touchstart', (e) => this.onPointerDown(e), { passive: false });
        this.overlayElement.addEventListener('touchmove', (e) => this.onPointerMove(e), { passive: false });
        this.overlayElement.addEventListener('touchend', (e) => this.onPointerUp(e));
        this.overlayElement.addEventListener('touchcancel', (e) => this.onPointerUp(e));
    }

    async loadInfo() {
        try {
            const response = await fetch('/api/info');
            const info = await response.json();

            this.width = info.width;
            this.height = info.height;
            this.fps = info.fps || 30;
            this.calibrationEnabled = info.calibration_enabled;

            document.getElementById('version').textContent = 'v' + info.version;
            document.getElementById('resolution').textContent = `${info.width}×${info.height} @ ${this.fps}fps`;
            document.getElementById('calibration-enabled').checked = info.calibration_enabled;
        } catch (error) {
            console.error('Failed to load info:', error);
            this.setStatus('Error', 'error');
        }
    }

    async loadCalibration() {
        try {
            const response = await fetch('/api/calibration');
            const data = await response.json();

            this.width = data.width;
            this.height = data.height;
            this.calibrationEnabled = data.enabled;

            document.getElementById('calibration-enabled').checked = data.enabled;

            this.corners = data.points.filter(p => p.point_type === 'corner').sort((a, b) => a.id - b.id);
            this.edgePoints = data.points.filter(p => p.point_type === 'edge');

            this.renderPoints();
        } catch (error) {
            console.error('Failed to load calibration:', error);
            this.showToast('Failed to load calibration', 'error');
        }
    }

    renderPoints() {
        const pointsGroup = document.getElementById('points-group');
        const gridLines = document.getElementById('grid-lines');

        pointsGroup.innerHTML = '';
        gridLines.innerHTML = '';

        if (this.corners.length < 4) return;

        this.renderGridLines(gridLines);

        this.corners.forEach((point, index) => {
            this.renderPoint(pointsGroup, point, index + 1, true);
        });

        this.edgePoints.forEach(point => {
            this.renderPoint(pointsGroup, point, null, false);
        });
    }

    renderPoint(container, point, label, isCorner) {
        const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        g.setAttribute('class', 'calibration-point ' + (isCorner ? 'corner' : 'edge'));
        g.setAttribute('data-id', point.id);

        const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        circle.setAttribute('cx', (point.x * 100) + '%');
        circle.setAttribute('cy', (point.y * 100) + '%');
        circle.setAttribute('r', isCorner ? 10 : 7);

        g.appendChild(circle);

        if (label) {
            const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
            text.setAttribute('x', (point.x * 100) + '%');
            text.setAttribute('y', (point.y * 100) + '%');
            text.setAttribute('dy', '0.35em');
            text.textContent = label.toString();
            g.appendChild(text);
        }

        container.appendChild(g);
    }

    renderGridLines(container) {
        if (this.corners.length < 4) return;

        this.edgeCorners.forEach((cornerPair, edgeIndex) => {
            const from = this.corners[cornerPair[0]];
            const to = this.corners[cornerPair[1]];
            if (!from || !to) return;

            const edgePts = this.edgePoints
                .filter(p => p.edge === edgeIndex)
                .sort((a, b) => {
                    const distA = Math.hypot(a.x - from.x, a.y - from.y);
                    const distB = Math.hypot(b.x - from.x, b.y - from.y);
                    return distA - distB;
                });

            const allPoints = [from, ...edgePts, to];

            for (let i = 0; i < allPoints.length - 1; i++) {
                const p1 = allPoints[i];
                const p2 = allPoints[i + 1];

                const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
                line.setAttribute('x1', (p1.x * 100) + '%');
                line.setAttribute('y1', (p1.y * 100) + '%');
                line.setAttribute('x2', (p2.x * 100) + '%');
                line.setAttribute('y2', (p2.y * 100) + '%');
                line.setAttribute('class', 'grid-line');
                container.appendChild(line);

                const clickLine = document.createElementNS('http://www.w3.org/2000/svg', 'line');
                clickLine.setAttribute('x1', (p1.x * 100) + '%');
                clickLine.setAttribute('y1', (p1.y * 100) + '%');
                clickLine.setAttribute('x2', (p2.x * 100) + '%');
                clickLine.setAttribute('y2', (p2.y * 100) + '%');
                clickLine.setAttribute('class', 'edge-segment');
                clickLine.setAttribute('data-edge', edgeIndex);
                container.appendChild(clickLine);
            }
        });
    }

    syncOverlaySize() {
        const img = this.previewElement;

        // Wait for image to be loaded and have dimensions
        if (!img.complete || !img.naturalWidth || !img.naturalHeight) return;

        // Get the actual rendered size of the image
        const imgRect = img.getBoundingClientRect();
        const wrapperRect = this.previewWrapper.getBoundingClientRect();

        // Calculate offset from wrapper to center the overlay on the image
        const offsetLeft = imgRect.left - wrapperRect.left;
        const offsetTop = imgRect.top - wrapperRect.top;

        // Set overlay to match exact image position and size
        this.overlayElement.style.width = imgRect.width + 'px';
        this.overlayElement.style.height = imgRect.height + 'px';
        this.overlayElement.style.left = offsetLeft + 'px';
        this.overlayElement.style.top = offsetTop + 'px';
    }

    // ========================================================================
    // Preview Control (Snapshot vs Live)
    // ========================================================================

    async captureSnapshot() {
        const timestamp = Date.now();
        const src = this.showCorrected
            ? '/api/preview?t=' + timestamp
            : '/api/preview/raw?t=' + timestamp;

        // Activate preview encoding temporarily to get a fresh frame
        try {
            await fetch('/api/preview/activate', { method: 'POST' });

            // Small delay to ensure a frame is encoded
            await new Promise(resolve => setTimeout(resolve, 150));

            const newImg = new Image();
            newImg.onload = () => {
                this.previewElement.src = newImg.src;
                // Sync overlay after image loads
                requestAnimationFrame(() => this.syncOverlaySize());
            };
            newImg.src = src;

            // If not in live mode, deactivate after capturing
            if (!this.livePreviewEnabled) {
                await new Promise(resolve => setTimeout(resolve, 100));
                await fetch('/api/preview/deactivate', { method: 'POST' });
            }
        } catch (error) {
            console.error('Failed to capture snapshot:', error);
        }
    }

    toggleLivePreview(enabled) {
        this.livePreviewEnabled = enabled;

        if (enabled) {
            this.startLivePreview();
            this.showToast('Live preview enabled', 'success');
        } else {
            this.stopLivePreview();
            this.showToast('Live preview disabled - using snapshots', 'success');
        }
    }

    async startLivePreview() {
        // Activate server-side encoding
        try {
            await fetch('/api/preview/activate', { method: 'POST' });
        } catch (error) {
            console.error('Failed to activate preview:', error);
        }

        // Start refresh interval
        if (!this.refreshInterval) {
            this.refreshInterval = setInterval(() => {
                this.refreshPreview();
            }, 100);
        }
    }

    stopLivePreview() {
        // Stop refresh interval
        if (this.refreshInterval) {
            clearInterval(this.refreshInterval);
            this.refreshInterval = null;
        }

        // Deactivate server-side encoding
        try {
            navigator.sendBeacon('/api/preview/deactivate');
        } catch (error) {
            // Fallback for browsers that don't support sendBeacon
            fetch('/api/preview/deactivate', { method: 'POST' }).catch(() => {});
        }
    }

    refreshPreview() {
        if (!this.livePreviewEnabled) return;

        const timestamp = Date.now();
        const src = this.showCorrected
            ? '/api/preview?t=' + timestamp
            : '/api/preview/raw?t=' + timestamp;

        const newImg = new Image();
        newImg.onload = () => {
            this.previewElement.src = newImg.src;
        };
        newImg.src = src;
    }

    onPointerDown(event) {
        const coords = this.getEventCoords(event);
        if (!coords) return;

        if (event.shiftKey) {
            const segment = event.target.closest('.edge-segment');
            if (segment) {
                event.preventDefault();
                const edgeIndex = parseInt(segment.getAttribute('data-edge'));
                this.addEdgePoint(edgeIndex, coords.x, coords.y);
                return;
            }
        }

        if (event.ctrlKey || event.metaKey) {
            const pointEl = event.target.closest('.calibration-point.edge');
            if (pointEl) {
                event.preventDefault();
                const id = parseInt(pointEl.getAttribute('data-id'));
                this.removeEdgePoint(id);
                return;
            }
        }

        const pointEl = event.target.closest('.calibration-point');
        if (!pointEl) return;

        event.preventDefault();

        const id = parseInt(pointEl.getAttribute('data-id'));

        this.draggingPoint = this.corners.find(p => p.id === id) ||
                             this.edgePoints.find(p => p.id === id);

        if (this.draggingPoint) {
            pointEl.classList.add('dragging');
        }
    }

    onPointerMove(event) {
        if (!this.draggingPoint) return;
        event.preventDefault();

        const coords = this.getEventCoords(event);
        if (!coords) return;

        this.draggingPoint.x = Math.max(0, Math.min(1, coords.x));
        this.draggingPoint.y = Math.max(0, Math.min(1, coords.y));

        this.renderPoints();
        this.throttledUpdatePoint(this.draggingPoint);
    }

    onPointerUp(event) {
        if (this.draggingPoint) {
            const element = this.overlayElement.querySelector('[data-id="' + this.draggingPoint.id + '"]');
            if (element) {
                element.classList.remove('dragging');
            }
            this.updatePoint(this.draggingPoint);
            this.draggingPoint = null;
        }
    }

    getEventCoords(event) {
        const rect = this.overlayElement.getBoundingClientRect();
        let clientX, clientY;

        if (event.touches && event.touches.length > 0) {
            clientX = event.touches[0].clientX;
            clientY = event.touches[0].clientY;
        } else {
            clientX = event.clientX;
            clientY = event.clientY;
        }

        return {
            x: (clientX - rect.left) / rect.width,
            y: (clientY - rect.top) / rect.height
        };
    }

    async addEdgePoint(edgeIndex, x, y) {
        try {
            const response = await fetch('/api/calibration/point/add', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ edge: edgeIndex, x: x, y: y })
            });

            if (response.ok) {
                await this.loadCalibration();
                this.showToast('Point added', 'success');
            } else {
                throw new Error('Failed to add point');
            }
        } catch (error) {
            console.error('Failed to add point:', error);
            this.showToast('Failed to add point', 'error');
        }
    }

    async removeEdgePoint(pointId) {
        try {
            const response = await fetch('/api/calibration/point/' + pointId, {
                method: 'DELETE'
            });

            if (response.ok) {
                await this.loadCalibration();
                this.showToast('Point removed', 'success');
            } else {
                throw new Error('Failed to remove point');
            }
        } catch (error) {
            console.error('Failed to remove point:', error);
            this.showToast('Failed to remove point', 'error');
        }
    }

    throttledUpdatePoint = (() => {
        let lastUpdate = 0;
        const minInterval = 50;

        return (point) => {
            const now = Date.now();
            if (now - lastUpdate >= minInterval) {
                lastUpdate = now;
                this.updatePoint(point);
            }
        };
    })();

    async updatePoint(point) {
        try {
            await fetch('/api/calibration/point/' + point.id, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ id: point.id, x: point.x, y: point.y })
            });
        } catch (error) {
            console.error('Failed to update point:', error);
        }
    }

    async toggleCalibration(enabled) {
        try {
            const endpoint = enabled ? '/api/calibration/enable' : '/api/calibration/disable';
            await fetch(endpoint, { method: 'POST' });
            this.calibrationEnabled = enabled;
        } catch (error) {
            console.error('Failed to toggle calibration:', error);
            this.showToast('Failed to toggle calibration', 'error');
        }
    }

    async resetCalibration() {
        try {
            await fetch('/api/calibration/reset', { method: 'POST' });
            await this.loadCalibration();
            this.showToast('Calibration reset', 'success');
        } catch (error) {
            console.error('Failed to reset calibration:', error);
            this.showToast('Failed to reset calibration', 'error');
        }
    }

    async saveCalibration() {
        try {
            const response = await fetch('/api/calibration/save', { method: 'POST' });
            if (response.ok) {
                this.showToast('Calibration saved!', 'success');
            } else {
                throw new Error('Save failed');
            }
        } catch (error) {
            console.error('Failed to save calibration:', error);
            this.showToast('Failed to save calibration', 'error');
        }
    }

    // ========================================================================
    // Camera Controls
    // ========================================================================

    toggleCameraPanel(show) {
        if (show === undefined) {
            show = !this.cameraPanelVisible;
        }
        this.cameraPanelVisible = show;

        if (show) {
            this.cameraPanelElement.classList.remove('hidden');
            this.loadCameraControls();
        } else {
            this.cameraPanelElement.classList.add('hidden');
        }
        // Re-sync overlay after layout change
        requestAnimationFrame(() => this.syncOverlaySize());
    }

    async loadCameraControls() {
        const loadingEl = document.getElementById('camera-controls-loading');
        const containerEl = document.getElementById('camera-controls-container');
        const unavailableEl = document.getElementById('camera-controls-unavailable');

        loadingEl.classList.remove('hidden');
        containerEl.classList.add('hidden');
        unavailableEl.classList.add('hidden');

        try {
            const response = await fetch('/api/camera/controls');
            const data = await response.json();

            loadingEl.classList.add('hidden');

            if (!data.available || data.controls.length === 0) {
                unavailableEl.classList.remove('hidden');
                return;
            }

            this.cameraControls = data.controls;
            this.renderCameraControls();
            containerEl.classList.remove('hidden');
        } catch (error) {
            console.error('Failed to load camera controls:', error);
            loadingEl.classList.add('hidden');
            unavailableEl.classList.remove('hidden');
        }
    }

    renderCameraControls() {
        const container = document.getElementById('camera-controls-container');
        container.innerHTML = '';

        // Group controls by category (based on ID ranges)
        const userControls = [];
        const cameraControls = [];

        for (const control of this.cameraControls) {
            // Skip disabled or inactive controls
            if (control.flags.disabled) continue;

            // Camera class controls have IDs starting with 0x009a
            if (control.id >= 0x009a0000 && control.id < 0x009b0000) {
                cameraControls.push(control);
            } else {
                userControls.push(control);
            }
        }

        // Render user controls
        if (userControls.length > 0) {
            const category = this.createControlCategory('Image Controls', userControls);
            container.appendChild(category);
        }

        // Render camera controls
        if (cameraControls.length > 0) {
            const category = this.createControlCategory('Camera Controls', cameraControls);
            container.appendChild(category);
        }
    }

    createControlCategory(title, controls) {
        const categoryEl = document.createElement('div');
        categoryEl.className = 'control-category';

        const titleEl = document.createElement('div');
        titleEl.className = 'control-category-title';
        titleEl.textContent = title;
        categoryEl.appendChild(titleEl);

        for (const control of controls) {
            const controlEl = this.createControlElement(control);
            categoryEl.appendChild(controlEl);
        }

        return categoryEl;
    }

    createControlElement(control) {
        const el = document.createElement('div');
        el.className = 'camera-control';
        el.dataset.controlId = control.id;

        if (control.flags.inactive) {
            el.classList.add('inactive');
        }

        const header = document.createElement('div');
        header.className = 'camera-control-header';

        const nameEl = document.createElement('span');
        nameEl.className = 'camera-control-name';
        nameEl.textContent = this.formatControlName(control.name);
        header.appendChild(nameEl);

        const valueEl = document.createElement('span');
        valueEl.className = 'camera-control-value';
        valueEl.id = 'camera-value-' + control.id;
        header.appendChild(valueEl);

        el.appendChild(header);

        // Create appropriate input based on control type
        switch (control.type) {
            case 'boolean':
                el.appendChild(this.createBooleanControl(control, valueEl));
                break;
            case 'menu':
            case 'integermenu':
                el.appendChild(this.createMenuControl(control, valueEl));
                break;
            case 'integer':
            case 'integer64':
            default:
                el.appendChild(this.createSliderControl(control, valueEl));
                break;
        }

        return el;
    }

    createSliderControl(control, valueEl) {
        const wrapper = document.createElement('div');

        const slider = document.createElement('input');
        slider.type = 'range';
        slider.className = 'camera-control-slider';
        slider.min = control.minimum;
        slider.max = control.maximum;
        slider.step = control.step || 1;
        slider.value = this.getControlValue(control);

        valueEl.textContent = slider.value;

        slider.addEventListener('input', (e) => {
            valueEl.textContent = e.target.value;
        });

        slider.addEventListener('change', (e) => {
            this.setCameraControl(control.id, parseInt(e.target.value));
        });

        wrapper.appendChild(slider);

        // Add min/max labels
        const metaEl = document.createElement('div');
        metaEl.className = 'camera-control-meta';
        metaEl.innerHTML = `<span>${control.minimum}</span><span>Default: ${control.default}</span><span>${control.maximum}</span>`;
        wrapper.appendChild(metaEl);

        return wrapper;
    }

    createBooleanControl(control, valueEl) {
        const wrapper = document.createElement('div');
        wrapper.className = 'camera-control-toggle';

        const toggle = document.createElement('label');
        toggle.className = 'toggle';

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = this.getControlValue(control) === true || this.getControlValue(control) === 1;

        valueEl.textContent = checkbox.checked ? 'On' : 'Off';

        checkbox.addEventListener('change', (e) => {
            valueEl.textContent = e.target.checked ? 'On' : 'Off';
            this.setCameraControl(control.id, e.target.checked);
        });

        const slider = document.createElement('span');
        slider.className = 'toggle-slider';

        toggle.appendChild(checkbox);
        toggle.appendChild(slider);
        wrapper.appendChild(toggle);

        return wrapper;
    }

    createMenuControl(control, valueEl) {
        const select = document.createElement('select');
        select.className = 'camera-control-select';

        if (control.menu_items) {
            for (const item of control.menu_items) {
                const option = document.createElement('option');
                option.value = item.index;
                option.textContent = item.label;
                select.appendChild(option);
            }
        }

        const currentValue = this.getControlValue(control);
        select.value = currentValue;

        const selectedOption = select.options[select.selectedIndex];
        valueEl.textContent = selectedOption ? selectedOption.textContent : currentValue;

        select.addEventListener('change', (e) => {
            const selectedOpt = e.target.options[e.target.selectedIndex];
            valueEl.textContent = selectedOpt ? selectedOpt.textContent : e.target.value;
            this.setCameraControl(control.id, parseInt(e.target.value));
        });

        return select;
    }

    getControlValue(control) {
        if (control.value === null || control.value === undefined) {
            return control.default;
        }
        // Handle different value types
        if (typeof control.value === 'object') {
            if ('Integer' in control.value) return control.value.Integer;
            if ('Boolean' in control.value) return control.value.Boolean;
            if ('String' in control.value) return control.value.String;
        }
        return control.value;
    }

    formatControlName(name) {
        // Convert snake_case to Title Case
        return name
            .replace(/_/g, ' ')
            .replace(/\b\w/g, c => c.toUpperCase());
    }

    async setCameraControl(id, value) {
        try {
            const response = await fetch('/api/camera/control/' + id, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ value: value })
            });

            if (!response.ok) {
                throw new Error('Failed to set control');
            }
        } catch (error) {
            console.error('Failed to set camera control:', error);
            this.showToast('Failed to set camera control', 'error');
        }
    }

    async resetCameraControls() {
        try {
            const response = await fetch('/api/camera/controls/reset', { method: 'POST' });
            if (response.ok) {
                await this.loadCameraControls();
                this.showToast('Camera controls reset', 'success');
            } else {
                throw new Error('Reset failed');
            }
        } catch (error) {
            console.error('Failed to reset camera controls:', error);
            this.showToast('Failed to reset camera controls', 'error');
        }
    }

    async refreshCameraControls() {
        try {
            await fetch('/api/camera/controls/refresh', { method: 'POST' });
            await this.loadCameraControls();
            this.showToast('Camera controls refreshed', 'success');
        } catch (error) {
            console.error('Failed to refresh camera controls:', error);
            this.showToast('Failed to refresh camera controls', 'error');
        }
    }

    // ========================================================================
    // Video Settings (Resolution/Framerate)
    // ========================================================================

    async loadVideoSettings() {
        const loadingEl = document.getElementById('video-settings-loading');
        const contentEl = document.getElementById('video-settings-content');
        const unavailableEl = document.getElementById('video-settings-unavailable');

        loadingEl.classList.remove('hidden');
        contentEl.classList.add('hidden');
        unavailableEl.classList.add('hidden');

        try {
            const response = await fetch('/api/video/capabilities');
            if (!response.ok) {
                throw new Error('Failed to fetch video capabilities');
            }

            const data = await response.json();
            this.videoCapabilities = data.capabilities;

            // Store current settings
            this.width = data.current.width;
            this.height = data.current.height;
            this.fps = data.current.fps;
            this.selectedResolution = `${data.current.width}x${data.current.height}`;
            this.selectedFps = data.current.fps;

            loadingEl.classList.add('hidden');

            if (!this.videoCapabilities || this.videoCapabilities.resolutions.length === 0) {
                unavailableEl.classList.remove('hidden');
                return;
            }

            this.renderVideoSettings();
            contentEl.classList.remove('hidden');

            // Check for pending settings
            await this.checkPendingSettings();
        } catch (error) {
            console.error('Failed to load video settings:', error);
            loadingEl.classList.add('hidden');
            unavailableEl.classList.remove('hidden');
        }
    }

    renderVideoSettings() {
        const resolutionSelect = document.getElementById('resolution-select');
        const fpsSelect = document.getElementById('fps-select');
        const currentResEl = document.getElementById('current-resolution');
        const currentFpsEl = document.getElementById('current-fps');

        // Update current values display
        currentResEl.textContent = `${this.width}×${this.height}`;
        currentFpsEl.textContent = `${this.fps} fps`;

        // Populate resolution dropdown
        resolutionSelect.innerHTML = '';
        const resolutions = this.videoCapabilities.resolutions;

        // Sort resolutions by total pixels (descending)
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

        // Populate FPS dropdown for current resolution
        this.updateFpsOptions();
    }

    updateFpsOptions() {
        const fpsSelect = document.getElementById('fps-select');
        fpsSelect.innerHTML = '';

        // Find framerates for selected resolution
        const [width, height] = this.selectedResolution.split('x').map(Number);
        const resInfo = this.videoCapabilities.resolutions.find(
            r => r.width === width && r.height === height
        );

        if (!resInfo || !resInfo.framerates || resInfo.framerates.length === 0) {
            // No specific framerates, add common defaults
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

        // Sort framerates descending
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

        // If current FPS not found, select first option
        if (!fpsSelect.value) {
            fpsSelect.selectedIndex = 0;
            this.selectedFps = parseInt(fpsSelect.value);
        }
    }

    onResolutionChange(value) {
        this.selectedResolution = value;
        this.updateFpsOptions();
        this.updatePendingState();
    }

    onFpsChange(value) {
        this.selectedFps = value;
        this.updatePendingState();
    }

    updatePendingState() {
        const [selectedWidth, selectedHeight] = this.selectedResolution.split('x').map(Number);
        const hasChanges =
            selectedWidth !== this.width ||
            selectedHeight !== this.height ||
            this.selectedFps !== this.fps;

        const noticeEl = document.getElementById('pending-settings-notice');
        const applyBtn = document.getElementById('apply-video-settings-btn');

        if (hasChanges) {
            noticeEl.classList.remove('hidden');
            applyBtn.classList.remove('hidden');
        } else {
            noticeEl.classList.add('hidden');
            applyBtn.classList.add('hidden');
        }
    }

    async checkPendingSettings() {
        try {
            const response = await fetch('/api/video/settings');
            const data = await response.json();

            if (data.pending) {
                this.pendingVideoSettings = data.pending;

                // Update selection to show pending values
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

    async applyVideoSettings() {
        const [width, height] = this.selectedResolution.split('x').map(Number);
        const fps = this.selectedFps;

        const applyBtn = document.getElementById('apply-video-settings-btn');
        applyBtn.disabled = true;
        applyBtn.textContent = 'Applying...';

        try {
            // Save the settings
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
                this.showToast('Settings saved. Restarting service...', 'success');

                // Request restart
                const restartResponse = await fetch('/api/system/restart', { method: 'POST' });
                const restartResult = await restartResponse.json();

                if (restartResult.success) {
                    // Show a message that we're restarting
                    this.setStatus('Restarting...', 'warning');
                    applyBtn.textContent = 'Restarting...';

                    // Wait for reconnection
                    this.waitForReconnect();
                } else {
                    throw new Error(restartResult.message || 'Failed to restart');
                }
            } else {
                this.showToast(saveResult.message, 'success');
                applyBtn.disabled = false;
                applyBtn.textContent = 'Apply & Restart';
                this.updatePendingState();
            }
        } catch (error) {
            console.error('Failed to apply video settings:', error);
            this.showToast('Failed to apply settings: ' + error.message, 'error');
            applyBtn.disabled = false;
            applyBtn.textContent = 'Apply & Restart';
        }
    }

    async waitForReconnect() {
        const maxAttempts = 30;
        const delayMs = 1000;
        let attempts = 0;

        this.setStatus('Reconnecting...', 'warning');

        const checkConnection = async () => {
            attempts++;
            try {
                const response = await fetch('/api/info', {
                    cache: 'no-store',
                    signal: AbortSignal.timeout(2000)
                });
                if (response.ok) {
                    // Reconnected! Reload the page to get fresh state
                    this.showToast('Service restarted successfully!', 'success');
                    setTimeout(() => {
                        window.location.reload();
                    }, 1000);
                    return;
                }
            } catch (error) {
                // Still down, keep trying
            }

            if (attempts < maxAttempts) {
                setTimeout(checkConnection, delayMs);
            } else {
                this.setStatus('Connection lost', 'error');
                this.showToast('Service may still be restarting. Try refreshing the page.', 'warning');
            }
        };

        // Start checking after a short delay
        setTimeout(checkConnection, 2000);
    }

    // ========================================================================
    // Preview & Performance Stats
    // ========================================================================

    startStatsRefresh() {
        this.statsInterval = setInterval(() => {
            this.loadStats();
            this.loadSystemStats();
        }, 1000);
    }

    async loadStats() {
        try {
            const response = await fetch('/api/stats');
            const stats = await response.json();
            this.updateStatsDisplay(stats);
        } catch (error) {
            // Silently fail for stats - non-critical
        }
    }

    async loadSystemStats() {
        try {
            const response = await fetch('/api/system/stats');
            const stats = await response.json();
            this.updateSystemStatsDisplay(stats);
        } catch (error) {
            // Silently fail for stats - non-critical
        }
    }

    updateSystemStatsDisplay(stats) {
        // CPU Temperature
        const cpuTempEl = document.getElementById('stat-cpu-temp');
        const cpuTempMini = document.getElementById('cpu-temp-mini');
        if (stats.cpu_temp_c !== null) {
            const temp = stats.cpu_temp_c;
            cpuTempEl.textContent = temp.toFixed(1) + ' °C';
            cpuTempMini.textContent = '🌡️ ' + temp.toFixed(0) + '°C';
            // Color code temperature
            if (temp >= 80) {
                cpuTempEl.className = 'stat-value temp-critical';
                cpuTempMini.className = 'mini-stat temp-critical';
            } else if (temp >= 70) {
                cpuTempEl.className = 'stat-value temp-warning';
                cpuTempMini.className = 'mini-stat temp-warning';
            } else {
                cpuTempEl.className = 'stat-value temp-ok';
                cpuTempMini.className = 'mini-stat temp-ok';
            }
        } else {
            cpuTempEl.textContent = 'N/A';
            cpuTempEl.className = 'stat-value';
            cpuTempMini.textContent = '🌡️ --°C';
            cpuTempMini.className = 'mini-stat';
        }

        // Memory usage
        const memEl = document.getElementById('stat-memory');
        const memMini = document.getElementById('memory-mini');
        memEl.textContent = stats.mem_used_percent.toFixed(0) + '%';
        memMini.textContent = '💾 ' + stats.mem_used_percent.toFixed(0) + '%';
        if (stats.mem_used_percent >= 90) {
            memEl.className = 'stat-value temp-critical';
            memMini.className = 'mini-stat temp-critical';
        } else if (stats.mem_used_percent >= 75) {
            memEl.className = 'stat-value temp-warning';
            memMini.className = 'mini-stat temp-warning';
        } else {
            memEl.className = 'stat-value';
            memMini.className = 'mini-stat';
        }

        // Load average
        const loadEl = document.getElementById('stat-load');
        const loadMini = document.getElementById('load-mini');
        if (stats.load_avg_1m !== null) {
            loadEl.textContent = stats.load_avg_1m.toFixed(2);
            loadMini.textContent = '📈 ' + stats.load_avg_1m.toFixed(1);
            // Color based on load (assuming 4 cores on Pi)
            if (stats.load_avg_1m >= 4) {
                loadEl.className = 'stat-value temp-critical';
                loadMini.className = 'mini-stat temp-critical';
            } else if (stats.load_avg_1m >= 2) {
                loadEl.className = 'stat-value temp-warning';
                loadMini.className = 'mini-stat temp-warning';
            } else {
                loadEl.className = 'stat-value';
                loadMini.className = 'mini-stat';
            }
        } else {
            loadEl.textContent = 'N/A';
            loadMini.textContent = '📈 --';
        }

        // Throttle status
        const throttleEl = document.getElementById('stat-throttle');
        if (stats.throttled) {
            const t = stats.throttled;
            let status = [];
            let isCritical = false;
            let isWarning = false;

            // Current issues (critical)
            if (t.under_voltage) { status.push('UV!'); isCritical = true; }
            if (t.throttled) { status.push('THR!'); isCritical = true; }
            if (t.freq_capped) { status.push('CAP'); isWarning = true; }
            if (t.soft_temp_limit) { status.push('HOT'); isWarning = true; }

            // Past issues (warning indicator)
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
        } else {
            throttleEl.textContent = '--';
            throttleEl.className = 'stat-value';
            throttleEl.title = 'Throttle status not available';
        }
    }

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

    updateStatsDisplay(stats) {
        // Update footer stats
        const fpsEl = document.getElementById('fps-display');
        const latencyEl = document.getElementById('latency-display');

        fpsEl.textContent = stats.fps.toFixed(1) + ' fps';
        // Show processing time (what we can optimize) in footer
        latencyEl.textContent = stats.timing.avg_processing_ms.toFixed(2) + ' ms';

        // Update detailed stats panel - separate hardware wait from processing
        document.getElementById('stat-frame-wait').textContent = stats.timing.avg_frame_wait_ms.toFixed(2) + ' ms';
        document.getElementById('stat-decode').textContent = stats.timing.avg_decode_ms.toFixed(2) + ' ms';
        document.getElementById('stat-transform').textContent = stats.timing.avg_transform_ms.toFixed(2) + ' ms';
        document.getElementById('stat-output').textContent = stats.timing.avg_output_ms.toFixed(2) + ' ms';

        // Show preview stats only when preview is active, otherwise show N/A
        if (stats.preview_active && stats.preview_frames_encoded > 0) {
            document.getElementById('stat-preview').textContent = stats.timing.avg_preview_encode_ms.toFixed(2) + ' ms';
        } else {
            document.getElementById('stat-preview').textContent = 'N/A';
        }

        // Processing time (what we control) and total pipeline time
        document.getElementById('stat-processing').textContent = stats.timing.avg_processing_ms.toFixed(2) + ' ms';
        document.getElementById('stat-pipeline').textContent = stats.timing.avg_pipeline_ms.toFixed(2) + ' ms';

        document.getElementById('stat-frames').textContent = this.formatNumber(stats.frames_processed);

        // Update preview status indicator
        const previewStatusEl = document.getElementById('preview-status');
        if (stats.preview_active) {
            previewStatusEl.textContent = 'Encoding: Active';
            previewStatusEl.className = 'preview-status active';
        } else {
            previewStatusEl.textContent = 'Encoding: Off';
            previewStatusEl.className = 'preview-status inactive';
        }
    }

    formatNumber(num) {
        if (num >= 1000000) {
            return (num / 1000000).toFixed(1) + 'M';
        } else if (num >= 1000) {
            return (num / 1000).toFixed(1) + 'K';
        }
        return num.toString();
    }

    toggleStatsPanel() {
        this.statsPanelVisible = !this.statsPanelVisible;
        const panel = document.getElementById('stats-panel');
        if (this.statsPanelVisible) {
            panel.classList.remove('hidden');
        } else {
            panel.classList.add('hidden');
        }
        // Re-sync overlay after layout change
        requestAnimationFrame(() => this.syncOverlaySize());
    }

    async resetStats() {
        try {
            await fetch('/api/stats/reset', { method: 'POST' });
            this.showToast('Stats reset', 'success');
        } catch (error) {
            console.error('Failed to reset stats:', error);
            this.showToast('Failed to reset stats', 'error');
        }
    }

    async restartService() {
        if (!confirm('Restart HyperCalibrate service?\n\nThe page will reload automatically when the service is back up.')) {
            return;
        }

        try {
            this.showToast('Restarting service...', 'info');
            this.setStatus('Restarting...', 'warning');

            await fetch('/api/system/restart', { method: 'POST' });

            // Wait a moment then start polling for service to come back
            setTimeout(() => this.waitForRestart(), 2000);
        } catch (error) {
            console.error('Failed to restart service:', error);
            this.showToast('Failed to send restart request', 'error');
        }
    }

    async waitForRestart() {
        const maxAttempts = 30;  // 30 seconds max wait
        let attempts = 0;

        const checkService = async () => {
            attempts++;
            try {
                const response = await fetch('/api/info', {
                    method: 'GET',
                    cache: 'no-store'
                });
                if (response.ok) {
                    // Service is back!
                    this.showToast('Service restarted successfully', 'success');
                    window.location.reload();
                    return;
                }
            } catch (e) {
                // Still down, keep trying
            }

            if (attempts < maxAttempts) {
                this.setStatus(`Waiting for restart... (${attempts}s)`, 'warning');
                setTimeout(checkService, 1000);
            } else {
                this.setStatus('Restart timeout', 'error');
                this.showToast('Service did not come back up. Check the Pi.', 'error');
            }
        };

        checkService();
    }

    // ========================================================================
    // UI Helpers
    // ========================================================================

    setStatus(text, className) {
        this.statusElement.textContent = text;
        this.statusElement.className = 'status ' + className;
    }

    showToast(message, type) {
        type = type || 'success';
        const toast = document.getElementById('toast');
        toast.textContent = message;
        toast.className = 'toast ' + type + ' show';

        setTimeout(() => {
            toast.classList.remove('show');
        }, 3000);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    window.app = new HyperCalibrate();
});
