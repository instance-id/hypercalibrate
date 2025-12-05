/**
 * HyperCalibrate - Preview Module
 * Handles live preview and snapshot capture
 */

import { showToast } from './utils.js';

export class PreviewManager {
    constructor(app) {
        this.app = app;
        this.previewElement = null;
        this.overlayElement = null;
        this.previewWrapper = null;
        this.refreshInterval = null;
        this.liveEnabled = false;
        this.showCorrected = false;
    }

    /**
     * Initialize preview manager
     */
    init(previewElement, overlayElement, previewWrapper) {
        this.previewElement = previewElement;
        this.overlayElement = overlayElement;
        this.previewWrapper = previewWrapper;

        // Sync overlay when image loads
        this.previewElement?.addEventListener('load', () => {
            // Use requestAnimationFrame to ensure layout is complete
            requestAnimationFrame(() => {
                this.syncOverlaySize();
            });
        });

        // Sync on window resize with debounce
        let resizeTimeout;
        window.addEventListener('resize', () => {
            clearTimeout(resizeTimeout);
            resizeTimeout = setTimeout(() => {
                this.syncOverlaySize();
            }, 50);
        });

        // Sync on orientation change (important for mobile)
        window.addEventListener('orientationchange', () => {
            // Delay to let browser complete layout after orientation change
            setTimeout(() => {
                this.syncOverlaySize();
            }, 200);
        });

        // Use ResizeObserver for more robust overlay syncing
        if (typeof ResizeObserver !== 'undefined') {
            const resizeObserver = new ResizeObserver(() => {
                requestAnimationFrame(() => {
                    this.syncOverlaySize();
                });
            });
            if (this.previewWrapper) {
                resizeObserver.observe(this.previewWrapper);
            }
            if (this.previewElement) {
                resizeObserver.observe(this.previewElement);
            }
        }
    }

    /**
     * Capture a single snapshot
     */
    async capture() {
        const timestamp = Date.now();
        const src = this.showCorrected
            ? '/api/preview?t=' + timestamp
            : '/api/preview/raw?t=' + timestamp;

        try {
            await fetch('/api/preview/activate', { method: 'POST' });
            await new Promise(resolve => setTimeout(resolve, 150));

            const newImg = new Image();
            newImg.onload = () => {
                if (this.previewElement) {
                    this.previewElement.src = newImg.src;
                }
                requestAnimationFrame(() => this.syncOverlaySize());
            };
            newImg.src = src;

            if (!this.liveEnabled) {
                await new Promise(resolve => setTimeout(resolve, 100));
                await fetch('/api/preview/deactivate', { method: 'POST' });
            }
        } catch (error) {
            console.error('Failed to capture snapshot:', error);
        }
    }

    /**
     * Toggle live preview mode
     */
    toggle(enabled) {
        this.liveEnabled = enabled;

        if (enabled) {
            this.start();
            showToast('Live preview enabled', 'success');
        } else {
            this.stop();
            showToast('Live preview disabled - using snapshots', 'success');
        }
    }

    /**
     * Start live preview
     */
    async start() {
        try {
            await fetch('/api/preview/activate', { method: 'POST' });
        } catch (error) {
            console.error('Failed to activate preview:', error);
        }

        if (!this.refreshInterval) {
            this.refreshInterval = setInterval(() => {
                this.refresh();
            }, 100);
        }
    }

    /**
     * Stop live preview
     */
    stop() {
        if (this.refreshInterval) {
            clearInterval(this.refreshInterval);
            this.refreshInterval = null;
        }

        try {
            navigator.sendBeacon('/api/preview/deactivate');
        } catch (error) {
            fetch('/api/preview/deactivate', { method: 'POST' }).catch(() => {});
        }
    }

    /**
     * Refresh the preview image
     */
    refresh() {
        if (!this.liveEnabled) return;

        const timestamp = Date.now();
        const src = this.showCorrected
            ? '/api/preview?t=' + timestamp
            : '/api/preview/raw?t=' + timestamp;

        const newImg = new Image();
        newImg.onload = () => {
            if (this.previewElement) {
                this.previewElement.src = newImg.src;
            }
        };
        newImg.src = src;
    }

    /**
     * Set whether to show corrected or raw preview
     */
    setShowCorrected(corrected) {
        this.showCorrected = corrected;
        if (!this.liveEnabled) {
            this.capture();
        }
    }

    /**
     * Sync overlay size to match the preview image
     */
    syncOverlaySize() {
        const img = this.previewElement;
        if (!img || !img.complete || !img.naturalWidth || !img.naturalHeight) return;

        const imgRect = img.getBoundingClientRect();
        const wrapperRect = this.previewWrapper?.getBoundingClientRect();
        if (!wrapperRect) return;

        const offsetLeft = imgRect.left - wrapperRect.left;
        const offsetTop = imgRect.top - wrapperRect.top;

        if (this.overlayElement) {
            this.overlayElement.style.width = imgRect.width + 'px';
            this.overlayElement.style.height = imgRect.height + 'px';
            this.overlayElement.style.left = offsetLeft + 'px';
            this.overlayElement.style.top = offsetTop + 'px';
        }
    }

    /**
     * Handle page visibility changes
     */
    onVisibilityChange(hidden) {
        if (hidden) {
            this.stop();
        } else if (this.liveEnabled) {
            this.start();
        }
    }
}
