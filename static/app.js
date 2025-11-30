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
        this.calibrationEnabled = true;
        this.showCorrected = false;
        this.draggingPoint = null;
        this.previewElement = null;
        this.overlayElement = null;
        this.refreshInterval = null;

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
        this.statusElement = document.getElementById('status');

        this.setupEventListeners();
        await this.loadInfo();
        await this.loadCalibration();
        this.startPreviewRefresh();
        this.setStatus('Connected', 'connected');
    }

    setupEventListeners() {
        document.getElementById('calibration-enabled').addEventListener('change', (e) => {
            this.toggleCalibration(e.target.checked);
        });

        document.getElementById('show-corrected').addEventListener('change', (e) => {
            this.showCorrected = e.target.checked;
        });

        document.getElementById('reset-btn').addEventListener('click', () => {
            this.resetCalibration();
        });

        document.getElementById('save-btn').addEventListener('click', () => {
            this.saveCalibration();
        });

        this.previewElement.addEventListener('load', () => {
            this.syncOverlaySize();
        });

        window.addEventListener('resize', () => {
            this.syncOverlaySize();
        });

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
            this.calibrationEnabled = info.calibration_enabled;

            document.getElementById('version').textContent = 'v' + info.version;
            document.getElementById('resolution').textContent = info.width + '×' + info.height;
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
        const wrapper = this.previewWrapper;

        if (!img.complete || !img.naturalWidth) return;

        const containerWidth = wrapper.clientWidth;
        const containerHeight = wrapper.clientHeight;
        const imgAspect = img.naturalWidth / img.naturalHeight;
        const containerAspect = containerWidth / containerHeight;

        let displayWidth, displayHeight;
        if (imgAspect > containerAspect) {
            displayWidth = containerWidth;
            displayHeight = containerWidth / imgAspect;
        } else {
            displayHeight = containerHeight;
            displayWidth = containerHeight * imgAspect;
        }

        this.overlayElement.style.width = displayWidth + 'px';
        this.overlayElement.style.height = displayHeight + 'px';
        this.overlayElement.style.left = ((containerWidth - displayWidth) / 2) + 'px';
        this.overlayElement.style.top = ((containerHeight - displayHeight) / 2) + 'px';
    }

    startPreviewRefresh() {
        this.refreshInterval = setInterval(() => {
            this.refreshPreview();
        }, 100);
    }

    refreshPreview() {
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
