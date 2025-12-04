/**
 * HyperCalibrate - Calibration Module
 * Handles calibration point rendering, manipulation, and API communication
 */

import { showToast } from './utils.js';
import { debug } from './debug.js';

/**
 * Edge corner mappings:
 * - Edge 0: TL (0) -> TR (1) - Top
 * - Edge 1: TR (1) -> BR (2) - Right
 * - Edge 2: BR (2) -> BL (3) - Bottom
 * - Edge 3: BL (3) -> TL (0) - Left
 */
export const EDGE_CORNERS = [
    [0, 1],  // Top edge
    [1, 2],  // Right edge
    [2, 3],  // Bottom edge
    [3, 0],  // Left edge
];

export class CalibrationManager {
    constructor(app) {
        this.app = app;
        this.corners = [];
        this.edgePoints = [];
        this.width = 640;
        this.height = 480;
        this.enabled = true;
        this.draggingPoint = null;
        this.overlayElement = null;
        
        // Throttled update function
        this._lastUpdate = 0;
        this._updateInterval = 50;
    }

    /**
     * Initialize the calibration manager
     * @param {HTMLElement} overlayElement - SVG overlay element
     */
    init(overlayElement) {
        this.overlayElement = overlayElement;
    }

    /**
     * Load calibration data from server
     */
    async load() {
        try {
            const response = await fetch('/api/calibration');
            const data = await response.json();

            this.width = data.width;
            this.height = data.height;
            this.enabled = data.enabled;

            document.getElementById('calibration-enabled').checked = data.enabled;

            this.corners = data.points.filter(p => p.point_type === 'corner').sort((a, b) => a.id - b.id);
            this.edgePoints = data.points.filter(p => p.point_type === 'edge');

            this.renderPoints();
            return data;
        } catch (error) {
            console.error('Failed to load calibration:', error);
            showToast('Failed to load calibration', 'error');
            throw error;
        }
    }

    /**
     * Render all calibration points and grid lines
     */
    renderPoints() {
        const pointsGroup = document.getElementById('points-group');
        const gridLines = document.getElementById('grid-lines');

        if (!pointsGroup || !gridLines) return;

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

    /**
     * Render a single calibration point
     */
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

    /**
     * Update point position in-place without recreating DOM elements
     * Critical for touch dragging - recreating elements breaks touch tracking
     */
    updatePointPosition(point) {
        if (!this.overlayElement) return;
        
        const element = this.overlayElement.querySelector(`[data-id="${point.id}"]`);
        if (!element) return;

        const circle = element.querySelector('circle');
        if (circle) {
            circle.setAttribute('cx', (point.x * 100) + '%');
            circle.setAttribute('cy', (point.y * 100) + '%');
        }

        const text = element.querySelector('text');
        if (text) {
            text.setAttribute('x', (point.x * 100) + '%');
            text.setAttribute('y', (point.y * 100) + '%');
        }

        // Also update the grid lines connected to this point
        this.updateGridLines();
    }

    /**
     * Update grid lines without recreating DOM - just update positions
     */
    updateGridLines() {
        const gridLines = document.getElementById('grid-lines');
        if (!gridLines || this.corners.length < 4) return;

        const lines = gridLines.querySelectorAll('line.grid-line');
        const clickLines = gridLines.querySelectorAll('line.edge-segment');

        let lineIndex = 0;

        EDGE_CORNERS.forEach((cornerPair, edgeIndex) => {
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

                if (lines[lineIndex]) {
                    lines[lineIndex].setAttribute('x1', (p1.x * 100) + '%');
                    lines[lineIndex].setAttribute('y1', (p1.y * 100) + '%');
                    lines[lineIndex].setAttribute('x2', (p2.x * 100) + '%');
                    lines[lineIndex].setAttribute('y2', (p2.y * 100) + '%');
                }

                if (clickLines[lineIndex]) {
                    clickLines[lineIndex].setAttribute('x1', (p1.x * 100) + '%');
                    clickLines[lineIndex].setAttribute('y1', (p1.y * 100) + '%');
                    clickLines[lineIndex].setAttribute('x2', (p2.x * 100) + '%');
                    clickLines[lineIndex].setAttribute('y2', (p2.y * 100) + '%');
                }

                lineIndex++;
            }
        });
    }

    /**
     * Render grid lines between points
     */
    renderGridLines(container) {
        if (this.corners.length < 4) return;

        EDGE_CORNERS.forEach((cornerPair, edgeIndex) => {
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

    /**
     * Find a point by ID (corner or edge)
     */
    findPoint(id) {
        return this.corners.find(p => p.id === id) || 
               this.edgePoints.find(p => p.id === id);
    }

    /**
     * Check if a point is an edge point (not a corner)
     */
    isEdgePoint(id) {
        return this.edgePoints.some(p => p.id === id);
    }

    /**
     * Update a point's position on the server (throttled)
     */
    throttledUpdatePoint(point) {
        const now = Date.now();
        if (now - this._lastUpdate >= this._updateInterval) {
            this._lastUpdate = now;
            this.updatePoint(point);
        }
    }

    /**
     * Update a point's position on the server
     */
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

    /**
     * Add an edge point
     */
    async addEdgePoint(edgeIndex, x, y) {
        try {
            const response = await fetch('/api/calibration/point/add', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ edge: edgeIndex, x: x, y: y })
            });

            if (response.ok) {
                await this.load();
                showToast('Point added', 'success');
            } else {
                showToast('Failed to add point', 'error');
            }
        } catch (error) {
            console.error('Failed to add point:', error);
            showToast('Failed to add point', 'error');
        }
    }

    /**
     * Remove an edge point
     */
    async removeEdgePoint(pointId) {
        try {
            const response = await fetch('/api/calibration/point/' + pointId, {
                method: 'DELETE'
            });

            if (response.ok) {
                await this.load();
                showToast('Point removed', 'success');
            } else {
                showToast('Failed to remove point', 'error');
            }
        } catch (error) {
            console.error('Failed to remove point:', error);
            showToast('Failed to remove point', 'error');
        }
    }

    /**
     * Toggle calibration enabled/disabled
     */
    async toggle(enabled) {
        try {
            const endpoint = enabled ? '/api/calibration/enable' : '/api/calibration/disable';
            await fetch(endpoint, { method: 'POST' });
            this.enabled = enabled;
        } catch (error) {
            console.error('Failed to toggle calibration:', error);
            showToast('Failed to toggle calibration', 'error');
        }
    }

    /**
     * Reset calibration to default
     */
    async reset() {
        try {
            await fetch('/api/calibration/reset', { method: 'POST' });
            await this.load();
            showToast('Calibration reset', 'success');
        } catch (error) {
            console.error('Failed to reset calibration:', error);
            showToast('Failed to reset calibration', 'error');
        }
    }

    /**
     * Save calibration to file
     */
    async save() {
        try {
            const response = await fetch('/api/calibration/save', { method: 'POST' });
            if (response.ok) {
                showToast('Calibration saved', 'success');
            } else {
                showToast('Failed to save calibration', 'error');
            }
        } catch (error) {
            console.error('Failed to save calibration:', error);
            showToast('Failed to save calibration', 'error');
        }
    }

    /**
     * Start dragging a point
     */
    startDrag(point) {
        this.draggingPoint = point;
        const element = this.overlayElement?.querySelector(`[data-id="${point.id}"]`);
        if (element) {
            element.classList.add('dragging');
        }
    }

    /**
     * Stop dragging
     */
    stopDrag() {
        if (this.draggingPoint) {
            const element = this.overlayElement?.querySelector(`[data-id="${this.draggingPoint.id}"]`);
            if (element) {
                element.classList.remove('dragging');
            }
            this.updatePoint(this.draggingPoint);
            debug.log('hit', `Released point ${this.draggingPoint.id}`);
            this.draggingPoint = null;
        }
    }

    /**
     * Move the currently dragging point to new coordinates
     */
    moveDraggingPoint(x, y) {
        if (!this.draggingPoint) return;

        this.draggingPoint.x = Math.max(0, Math.min(1, x));
        this.draggingPoint.y = Math.max(0, Math.min(1, y));

        debug.log('event', `dragging to (${x.toFixed(3)}, ${y.toFixed(3)})`);

        this.updatePointPosition(this.draggingPoint);
        this.throttledUpdatePoint(this.draggingPoint);
    }
}
