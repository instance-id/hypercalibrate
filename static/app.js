/**
 * HyperCalibrate - Main Application
 *
 * TV screen calibration UI for Hyperion ambient lighting.
 *
 * Architecture:
 * - Modules are loaded via ES6 imports
 * - Each module handles a specific concern (calibration, touch, camera, etc.)
 * - This file initializes modules and wires up event listeners
 */

import { showToast, setStatus, isMobileDevice } from './js/utils.js';
import { DebugManager, debug } from './js/debug.js';
import { CalibrationManager } from './js/calibration.js';
import { TouchManager } from './js/touch.js';
import { CameraManager } from './js/camera.js';
import { VideoManager } from './js/video.js';
import { PreviewManager } from './js/preview.js';
import { StatsManager } from './js/stats.js';

class HyperCalibrate {
    constructor() {
        // Core state
        this.width = 640;
        this.height = 480;
        this.fps = 30;
        this.calibrationEnabled = true;

        // DOM elements
        this.previewElement = null;
        this.overlayElement = null;
        this.previewWrapper = null;
        this.previewContainer = null;
        this.statusElement = null;

        // Module instances
        this.calibration = new CalibrationManager(this);
        this.touch = new TouchManager(this);
        this.camera = new CameraManager(this);
        this.video = new VideoManager(this);
        this.preview = new PreviewManager(this);
        this.stats = new StatsManager(this);
        this.debug = debug;

        // Device detection
        this.isMobile = isMobileDevice();

        this.init();
    }

    async init() {
        // Get DOM elements
        this.previewElement = document.getElementById('preview');
        this.overlayElement = document.getElementById('calibration-overlay');
        this.previewWrapper = document.getElementById('preview-wrapper');
        this.previewContainer = document.querySelector('.preview-container');
        this.statusElement = document.getElementById('status');

        // Initialize modules
        this.debug.init(document.getElementById('debug-log'));
        this.calibration.init(this.overlayElement);
        this.preview.init(this.previewElement, this.overlayElement, this.previewWrapper);
        this.touch.init(this.overlayElement, this.calibration);
        this.camera.init(document.getElementById('camera-panel'));

        // Set up event listeners
        this.setupEventListeners();

        // Load initial data
        await this.loadInfo();
        await this.calibration.load();
        await this.video.load();
        await this.camera.load();

        // Capture initial snapshot
        await this.preview.capture();

        // Start stats refresh
        this.stats.start();
        setStatus('Connected', 'connected');

        // Handle page visibility changes
        document.addEventListener('visibilitychange', () => {
            this.preview.onVisibilityChange(document.hidden);
        });

        // Cleanup on page unload
        window.addEventListener('beforeunload', () => {
            this.preview.stop();
        });
    }

    setupEventListeners() {
        // Calibration controls
        document.getElementById('calibration-enabled')?.addEventListener('change', (e) => {
            this.calibration.toggle(e.target.checked);
            this.calibrationEnabled = e.target.checked;
        });

        document.getElementById('show-corrected')?.addEventListener('change', (e) => {
            this.preview.setShowCorrected(e.target.checked);
        });

        document.getElementById('reset-btn')?.addEventListener('click', () => {
            this.calibration.reset();
        });

        document.getElementById('save-btn')?.addEventListener('click', () => {
            this.calibration.save();
        });

        // Preview controls
        document.getElementById('capture-snapshot-btn')?.addEventListener('click', () => {
            this.preview.capture();
        });

        document.getElementById('live-preview-enabled')?.addEventListener('change', (e) => {
            this.preview.toggle(e.target.checked);
        });

        // Camera panel controls
        document.getElementById('toggle-camera-panel')?.addEventListener('click', () => {
            const visible = this.camera.toggle();
            requestAnimationFrame(() => this.preview.syncOverlaySize());
        });

        document.getElementById('close-camera-panel')?.addEventListener('click', () => {
            this.camera.toggle(false);
            requestAnimationFrame(() => this.preview.syncOverlaySize());
        });

        document.getElementById('reset-camera-btn')?.addEventListener('click', () => {
            this.camera.reset();
        });

        document.getElementById('refresh-camera-btn')?.addEventListener('click', () => {
            this.camera.refresh();
        });

        // Video settings controls
        document.getElementById('resolution-select')?.addEventListener('change', (e) => {
            this.video.onResolutionChange(e.target.value);
        });

        document.getElementById('fps-select')?.addEventListener('change', (e) => {
            this.video.onFpsChange(parseInt(e.target.value));
        });

        document.getElementById('apply-video-settings-btn')?.addEventListener('click', () => {
            this.video.apply();
        });

        // Stats panel controls
        document.getElementById('toggle-stats')?.addEventListener('click', () => {
            this.stats.toggle();
            requestAnimationFrame(() => this.preview.syncOverlaySize());
        });

        document.getElementById('reset-stats-btn')?.addEventListener('click', () => {
            this.stats.reset();
        });

        document.getElementById('restart-service-btn')?.addEventListener('click', () => {
            this.stats.restart();
        });

        // Debug panel controls
        document.getElementById('toggle-debug')?.addEventListener('click', () => {
            this.debug.toggle();
        });

        document.getElementById('debug-clear')?.addEventListener('click', () => {
            this.debug.clearLog();
        });

        document.getElementById('debug-select-all')?.addEventListener('click', () => {
            this.debug.selectAll();
        });

        // Mouse events for overlay (desktop)
        this.overlayElement?.addEventListener('mousedown', (e) => this.onPointerDown(e));
        this.overlayElement?.addEventListener('mousemove', (e) => this.onPointerMove(e));
        this.overlayElement?.addEventListener('mouseup', (e) => this.onPointerUp(e));
        this.overlayElement?.addEventListener('mouseleave', (e) => this.onPointerUp(e));

        // Prevent context menu on overlay
        this.overlayElement?.addEventListener('contextmenu', (e) => e.preventDefault());
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
            setStatus('Error', 'error');
        }
    }

    // Desktop mouse event handlers (touch is handled by TouchManager)
    onPointerDown(event) {
        if (this.touch?.touchActive) return;

        const coords = this.getEventCoords(event);
        if (!coords) return;

        // Shift+click to add point
        if (event.shiftKey) {
            const segment = event.target.closest('.edge-segment');
            if (segment) {
                event.preventDefault();
                const edgeIndex = parseInt(segment.getAttribute('data-edge'));
                this.calibration.addEdgePoint(edgeIndex, coords.x, coords.y);
                return;
            }
        }

        // Ctrl+click to remove point
        if (event.ctrlKey || event.metaKey) {
            const pointEl = event.target.closest('.calibration-point.edge');
            if (pointEl) {
                event.preventDefault();
                const id = parseInt(pointEl.getAttribute('data-id'));
                this.calibration.removeEdgePoint(id);
                return;
            }
        }

        // Start dragging point
        const pointEl = event.target.closest('.calibration-point');
        if (!pointEl) return;

        event.preventDefault();

        const id = parseInt(pointEl.getAttribute('data-id'));
        const point = this.calibration.findPoint(id);

        if (point) {
            this.calibration.startDrag(point);
        }
    }

    onPointerMove(event) {
        if (this.touch?.touchActive) return;
        if (!this.calibration.draggingPoint) return;

        event.preventDefault();

        const coords = this.getEventCoords(event);
        if (!coords) return;

        this.calibration.moveDraggingPoint(coords.x, coords.y);
    }

    onPointerUp(event) {
        if (this.touch?.touchActive) return;
        this.calibration.stopDrag();
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
}

// Initialize app when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    window.app = new HyperCalibrate();
});
