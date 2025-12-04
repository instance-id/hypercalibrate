/**
 * HyperCalibrate - Touch Handling Module
 * Handles all touch/mouse input including dragging and long-press for edge point management
 */

import { debug } from './debug.js';
import { EDGE_CORNERS } from './calibration.js';

export class TouchManager {
    constructor(app) {
        this.app = app;
        this.calibrationManager = null;

        // Touch state
        this.touchStartTime = 0;
        this.touchStartPos = null;
        this.touchActive = false;
        this.longPressTimer = null;
        this.longPressTarget = null;
        this.isDragging = false;
        this.longPressFired = false;

        // Long press configuration
        this.LONG_PRESS_DURATION = 500;
        this.DRAG_THRESHOLD = 10;

        this.overlayElement = null;
    }

    /**
     * Initialize touch handling on the overlay element
     */
    init(overlayElement, calibrationManager) {
        this.overlayElement = overlayElement;
        this.calibrationManager = calibrationManager;

        this.bindEvents();
    }

    /**
     * Bind all touch/mouse events
     */
    bindEvents() {
        if (!this.overlayElement) return;

        // Touch events with passive: false for preventDefault
        // Use window with capture phase to intercept ALL touch events
        this.overlayElement.addEventListener('touchstart', (e) => this.onTouchStart(e), { passive: false });
        window.addEventListener('touchmove', (e) => this.onTouchMove(e), { passive: false, capture: true });
        window.addEventListener('touchend', (e) => this.onTouchEnd(e), { capture: true });
        window.addEventListener('touchcancel', (e) => this.onTouchEnd(e), { capture: true });

        // Context menu for right-click (desktop)
        this.overlayElement.addEventListener('contextmenu', (e) => this.handleContextMenu(e));
    }

    /**
     * Handle touch start - with generous touch radius for mobile
     */
    onTouchStart(e) {
        debug.log('event', `touchstart - touches: ${e.touches.length}`);

        if (e.touches.length !== 1) return;

        // CRITICAL: Prevent default immediately to stop browser scroll/pan
        e.preventDefault();

        const touch = e.touches[0];
        debug.log('coords', `client: (${Math.round(touch.clientX)}, ${Math.round(touch.clientY)})`);

        // Clear any previous drag state
        if (this.calibrationManager) {
            this.calibrationManager.draggingPoint = null;
        }
        this.cancelLongPress();

        // Get normalized coordinates (0-1)
        const coords = this.getRelativeCoords(e);
        if (!coords) {
            debug.log('miss', 'getRelativeCoords returned null');
            return;
        }
        debug.log('coords', `normalized: (${coords.x.toFixed(3)}, ${coords.y.toFixed(3)})`);

        this.touchStartTime = Date.now();
        this.touchStartPos = { x: touch.clientX, y: touch.clientY };
        this.isDragging = false;
        this.longPressFired = false;
        this.touchActive = true;

        // Calculate touch radius for hit detection
        const rect = this.overlayElement.getBoundingClientRect();
        const minPixelRadius = 50;
        const touchRadiusPixels = Math.max(minPixelRadius, Math.min(rect.width, rect.height) * 0.15);
        const touchRadius = touchRadiusPixels / Math.min(rect.width, rect.height);
        debug.log('info', `touchRadius: ${touchRadius.toFixed(3)} (${Math.round(touchRadiusPixels)}px)`);

        // Find closest point within touch radius
        let closestPoint = null;
        let closestDist = touchRadius;
        let isEdgePoint = false;

        const corners = this.calibrationManager?.corners || [];
        const edgePoints = this.calibrationManager?.edgePoints || [];

        // Check corners
        for (const point of corners) {
            const dist = Math.hypot(coords.x - point.x, coords.y - point.y);
            if (dist < closestDist) {
                closestDist = dist;
                closestPoint = point;
                isEdgePoint = false;
            }
        }

        // Check edge points
        for (const point of edgePoints) {
            const dist = Math.hypot(coords.x - point.x, coords.y - point.y);
            if (dist < closestDist) {
                closestDist = dist;
                closestPoint = point;
                isEdgePoint = true;
            }
        }

        if (closestPoint) {
            debug.log('hit', `Found point ${closestPoint.id} at dist ${closestDist.toFixed(3)}`);
            this.calibrationManager?.startDrag(closestPoint);

            // Set up long-press for edge point removal
            if (isEdgePoint) {
                const pointEl = this.overlayElement.querySelector(`[data-id="${closestPoint.id}"]`);
                this.startLongPressTimer(pointEl, closestPoint.id, 'point');
            }
            return;
        }

        debug.log('miss', `No point within touchRadius`);

        // Check for edge segment hit
        const edgeHit = this.findClosestEdgeSegment(coords, touchRadius);
        if (edgeHit) {
            debug.log('hit', `Edge ${edgeHit.edgeIndex} at dist ${edgeHit.distance.toFixed(3)}`);
            const edgeEl = this.overlayElement.querySelector(`.edge-segment[data-edge="${edgeHit.edgeIndex}"]`);
            this.startLongPressTimer(edgeEl, { edgeIndex: edgeHit.edgeIndex, x: coords.x, y: coords.y }, 'edge');
        }
    }

    /**
     * Find closest edge segment to a point
     */
    findClosestEdgeSegment(coords, maxDistance) {
        const corners = this.calibrationManager?.corners || [];
        const edgePoints = this.calibrationManager?.edgePoints || [];

        let closestDist = maxDistance;
        let closestEdge = null;

        for (let edgeIndex = 0; edgeIndex < EDGE_CORNERS.length; edgeIndex++) {
            const [fromIdx, toIdx] = EDGE_CORNERS[edgeIndex];
            const from = corners[fromIdx];
            const to = corners[toIdx];
            if (!from || !to) continue;

            // Get all points on this edge in order
            const edgePts = edgePoints
                .filter(p => p.edge === edgeIndex)
                .sort((a, b) => {
                    const distA = Math.hypot(a.x - from.x, a.y - from.y);
                    const distB = Math.hypot(b.x - from.x, b.y - from.y);
                    return distA - distB;
                });
            const allPoints = [from, ...edgePts, to];

            // Check each segment
            for (let i = 0; i < allPoints.length - 1; i++) {
                const p1 = allPoints[i];
                const p2 = allPoints[i + 1];
                const dist = this.pointToSegmentDistance(coords.x, coords.y, p1.x, p1.y, p2.x, p2.y);

                if (dist < closestDist) {
                    closestDist = dist;
                    closestEdge = { edgeIndex, distance: dist };
                }
            }
        }

        return closestEdge;
    }

    /**
     * Calculate distance from point to line segment
     */
    pointToSegmentDistance(px, py, x1, y1, x2, y2) {
        const dx = x2 - x1;
        const dy = y2 - y1;
        const lengthSq = dx * dx + dy * dy;

        if (lengthSq === 0) {
            return Math.hypot(px - x1, py - y1);
        }

        let t = ((px - x1) * dx + (py - y1) * dy) / lengthSq;
        t = Math.max(0, Math.min(1, t));

        const projX = x1 + t * dx;
        const projY = y1 + t * dy;

        return Math.hypot(px - projX, py - projY);
    }

    /**
     * Handle touch move
     */
    onTouchMove(e) {
        if (!this.touchActive) return;

        if (this.calibrationManager?.draggingPoint) {
            e.preventDefault();
        }

        if (e.touches.length !== 1) {
            this.cancelLongPress();
            return;
        }

        const touch = e.touches[0];

        // Check if we've moved enough to cancel long-press
        if (this.touchStartPos) {
            const dx = touch.clientX - this.touchStartPos.x;
            const dy = touch.clientY - this.touchStartPos.y;
            const distance = Math.sqrt(dx * dx + dy * dy);

            if (distance > this.DRAG_THRESHOLD) {
                if (!this.isDragging) {
                    debug.log('event', `touchmove - moved ${Math.round(distance)}px, canceling long-press`);
                    this.isDragging = true;
                }
                this.cancelLongPress();
            }
        }

        // Handle dragging
        if (!this.calibrationManager?.draggingPoint) return;

        const coords = this.getRelativeCoords(e);
        if (!coords) return;

        this.calibrationManager.moveDraggingPoint(coords.x, coords.y);
    }

    /**
     * Handle touch end
     */
    onTouchEnd(e) {
        if (!this.touchActive) return;

        debug.log('event', `touchend - draggingPoint: ${this.calibrationManager?.draggingPoint?.id || 'null'}`);

        this.cancelLongPress();
        this.calibrationManager?.stopDrag();
        this.touchStartPos = null;

        // Clear touch active flag after delay to prevent synthesized mouse events
        setTimeout(() => {
            this.touchActive = false;
        }, 300);
    }

    /**
     * Handle context menu (right-click on desktop)
     */
    handleContextMenu(e) {
        const target = e.target;
        const pointElement = target?.closest('.calibration-point.edge');
        const edgeElement = target?.closest('.edge-segment');

        if (pointElement) {
            e.preventDefault();
            const pointId = parseInt(pointElement.dataset.id);
            debug.log('action', `Right-click remove point ${pointId}`);
            this.calibrationManager?.removeEdgePoint(pointId);
        } else if (edgeElement) {
            e.preventDefault();
            const edgeIndex = parseInt(edgeElement.dataset.edge);
            const coords = this.getRelativeCoords(e);
            debug.log('action', `Right-click add point on edge ${edgeIndex}`);
            this.calibrationManager?.addEdgePoint(edgeIndex, coords.x, coords.y);
        }
    }

    /**
     * Start the long press timer
     */
    startLongPressTimer(element, data, type) {
        // Cancel any existing timer first, but preserve target info
        if (this.longPressTimer) {
            clearTimeout(this.longPressTimer);
            this.longPressTimer = null;
        }

        this.longPressTarget = { element, data, type };

        debug.log('event', `Starting long press timer for ${type}`);

        this.longPressTimer = setTimeout(() => {
            if (this.longPressTarget && !this.isDragging) {
                this.handleLongPress(this.longPressTarget);
            }
        }, this.LONG_PRESS_DURATION);
    }

    /**
     * Cancel the long press timer
     */
    cancelLongPress() {
        if (this.longPressTimer) {
            clearTimeout(this.longPressTimer);
            this.longPressTimer = null;
        }
        this.longPressTarget = null;
    }

    /**
     * Handle long press action - show mobile menu
     */
    handleLongPress(target) {
        if (this.longPressFired) return;
        this.longPressFired = true;

        debug.log('action', `Long press on ${target.type}`);

        // Haptic feedback
        if (navigator.vibrate) {
            navigator.vibrate(50);
        }

        if (target.type === 'point') {
            this.showMobileMenu('remove', target);
        } else if (target.type === 'edge') {
            this.showMobileMenu('add', target);
        }

        // Cancel any drag
        if (this.calibrationManager?.draggingPoint) {
            this.calibrationManager.stopDrag();
        }

        this.longPressTarget = null;
    }

    /**
     * Show mobile action menu for add/remove point
     */
    showMobileMenu(action, target) {
        // Remove any existing menu
        const existingMenu = document.getElementById('mobile-point-menu');
        if (existingMenu) existingMenu.remove();

        const menu = document.createElement('div');
        menu.id = 'mobile-point-menu';
        menu.className = 'mobile-point-menu';

        if (action === 'add') {
            menu.innerHTML = `
                <button class="mobile-menu-btn add-btn">➕ Add Point Here</button>
                <button class="mobile-menu-btn cancel-btn">Cancel</button>
            `;
            menu.querySelector('.add-btn').addEventListener('click', () => {
                this.calibrationManager?.addEdgePoint(
                    target.data.edgeIndex,
                    target.data.x,
                    target.data.y
                );
                menu.remove();
            });
        } else if (action === 'remove') {
            menu.innerHTML = `
                <button class="mobile-menu-btn remove-btn">🗑️ Remove Point</button>
                <button class="mobile-menu-btn cancel-btn">Cancel</button>
            `;
            menu.querySelector('.remove-btn').addEventListener('click', () => {
                this.calibrationManager?.removeEdgePoint(target.data);
                menu.remove();
            });
        }

        menu.querySelector('.cancel-btn').addEventListener('click', () => menu.remove());

        // Close menu when clicking outside
        setTimeout(() => {
            document.addEventListener('click', function closeMenu(e) {
                if (!menu.contains(e.target)) {
                    menu.remove();
                    document.removeEventListener('click', closeMenu);
                }
            });
        }, 100);

        document.body.appendChild(menu);
    }

    /**
     * Get touch/mouse point from event
     */
    getEventPoint(e) {
        if (e.touches && e.touches.length > 0) {
            return e.touches[0];
        }
        return e;
    }

    /**
     * Get relative coordinates (0-1) from event
     */
    getRelativeCoords(e) {
        if (!this.overlayElement) return null;

        const point = this.getEventPoint(e);
        const rect = this.overlayElement.getBoundingClientRect();

        const x = (point.clientX - rect.left) / rect.width;
        const y = (point.clientY - rect.top) / rect.height;

        return { x, y };
    }
}
