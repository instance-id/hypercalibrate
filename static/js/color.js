/**
 * HyperCalibrate - Color Correction Module
 * Handles color space conversion and software color adjustments
 * for HDMI capture devices
 */

import { showToast } from './utils.js';

export class ColorManager {
    constructor(app) {
        this.app = app;
        this.settings = null;
        this.presets = [];
        this.panelVisible = false;
        this.panelElement = null;
    }

    /**
     * Initialize color manager
     */
    init(panelElement) {
        this.panelElement = panelElement;
    }

    /**
     * Toggle the color panel visibility
     */
    toggle(show) {
        if (show === undefined) {
            show = !this.panelVisible;
        }
        this.panelVisible = show;

        if (show) {
            this.panelElement?.classList.remove('hidden');
            document.body.classList.add('color-panel-open');
            this.load();
        } else {
            this.panelElement?.classList.add('hidden');
            document.body.classList.remove('color-panel-open');
        }

        return this.panelVisible;
    }

    /**
     * Load color settings and presets from server
     */
    async load() {
        try {
            const [settingsRes, presetsRes] = await Promise.all([
                fetch('/api/color'),
                fetch('/api/color/presets')
            ]);

            const settingsData = await settingsRes.json();
            this.settings = settingsData.settings;
            this.colorSpaces = settingsData.color_spaces;
            this.quantizationRanges = settingsData.quantization_ranges;

            this.presets = await presetsRes.json();

            this.render();
        } catch (error) {
            console.error('Failed to load color settings:', error);
            showToast('Failed to load color settings', 'error');
        }
    }

    /**
     * Render the color correction panel
     */
    render() {
        const container = document.getElementById('color-controls-container');
        if (!container) return;

        container.innerHTML = '';

        // Enable toggle
        const enableSection = this.createSection('Color Correction');
        enableSection.appendChild(this.createEnableToggle());
        container.appendChild(enableSection);

        // Presets section
        const presetsSection = this.createSection('Quick Presets');
        presetsSection.appendChild(this.createPresetsGrid());
        container.appendChild(presetsSection);

        // Color space settings
        const colorSpaceSection = this.createSection('Color Space');
        colorSpaceSection.appendChild(this.createColorSpaceSelect());
        colorSpaceSection.appendChild(this.createQuantizationSelect());
        container.appendChild(colorSpaceSection);

        // Software adjustments
        const adjustSection = this.createSection('Software Adjustments');
        adjustSection.appendChild(this.createSlider('brightness', 'Brightness', -100, 100, 1, 0));
        adjustSection.appendChild(this.createSlider('contrast', 'Contrast', 0, 2, 0.05, 1));
        adjustSection.appendChild(this.createSlider('saturation', 'Saturation', 0, 2, 0.05, 1));
        adjustSection.appendChild(this.createSlider('hue', 'Hue', -180, 180, 1, 0));
        adjustSection.appendChild(this.createSlider('gamma', 'Gamma', 0.1, 3, 0.05, 1));
        container.appendChild(adjustSection);

        // White Balance (RGB Gain)
        const wbSection = this.createSection('White Balance');
        wbSection.appendChild(this.createAutoWBButton());
        wbSection.appendChild(this.createSlider('red_gain', 'Red', 0.5, 2, 0.01, 1));
        wbSection.appendChild(this.createSlider('green_gain', 'Green', 0.5, 2, 0.01, 1));
        wbSection.appendChild(this.createSlider('blue_gain', 'Blue', 0.5, 2, 0.01, 1));
        container.appendChild(wbSection);

        // Reset button
        const resetBtn = document.createElement('button');
        resetBtn.className = 'color-reset-btn';
        resetBtn.textContent = '↺ Reset to Defaults';
        resetBtn.addEventListener('click', () => this.applyPreset('passthrough'));
        container.appendChild(resetBtn);

        this.updateControlStates();
    }

    /**
     * Create a section with title
     */
    createSection(title) {
        const section = document.createElement('div');
        section.className = 'color-section';

        const titleEl = document.createElement('div');
        titleEl.className = 'color-section-title';
        titleEl.textContent = title;
        section.appendChild(titleEl);

        return section;
    }

    /**
     * Create the enable/disable toggle
     */
    createEnableToggle() {
        const wrapper = document.createElement('div');
        wrapper.className = 'color-enable-wrapper';

        const label = document.createElement('span');
        label.textContent = 'Enable Color Correction';
        wrapper.appendChild(label);

        const toggle = document.createElement('label');
        toggle.className = 'toggle';

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = this.settings?.enabled || false;
        checkbox.addEventListener('change', (e) => {
            this.updateSetting('enabled', e.target.checked);
            this.updateControlStates();
        });

        const slider = document.createElement('span');
        slider.className = 'toggle-slider';

        toggle.appendChild(checkbox);
        toggle.appendChild(slider);
        wrapper.appendChild(toggle);

        return wrapper;
    }

    /**
     * Create presets grid
     */
    createPresetsGrid() {
        const grid = document.createElement('div');
        grid.className = 'color-presets-grid';

        for (const preset of this.presets) {
            const btn = document.createElement('button');
            btn.className = 'color-preset-btn';
            btn.innerHTML = `<span class="preset-name">${preset.name}</span>`;
            btn.title = preset.description;
            btn.addEventListener('click', () => this.applyPreset(preset.id));
            grid.appendChild(btn);
        }

        return grid;
    }

    /**
     * Create color space select
     */
    createColorSpaceSelect() {
        const wrapper = document.createElement('div');
        wrapper.className = 'color-control';

        const label = document.createElement('label');
        label.textContent = 'Color Matrix';
        label.className = 'color-control-label';
        wrapper.appendChild(label);

        const select = document.createElement('select');
        select.className = 'color-select';
        select.id = 'color-space-select';

        for (const cs of this.colorSpaces || []) {
            const option = document.createElement('option');
            option.value = cs.value;
            option.textContent = cs.label;
            if (this.settings?.color_space === cs.value) {
                option.selected = true;
            }
            select.appendChild(option);
        }

        select.addEventListener('change', (e) => {
            this.updateSetting('color_space', e.target.value);
        });

        wrapper.appendChild(select);

        const help = document.createElement('div');
        help.className = 'color-control-help';
        help.textContent = 'BT.709 for most HD content, BT.2020 for HDR/4K content';
        wrapper.appendChild(help);

        return wrapper;
    }

    /**
     * Create quantization range select
     */
    createQuantizationSelect() {
        const wrapper = document.createElement('div');
        wrapper.className = 'color-control';

        const label = document.createElement('label');
        label.textContent = 'Input Range';
        label.className = 'color-control-label';
        wrapper.appendChild(label);

        const select = document.createElement('select');
        select.className = 'color-select';
        select.id = 'quantization-select';

        for (const qr of this.quantizationRanges || []) {
            const option = document.createElement('option');
            option.value = qr.value;
            option.textContent = qr.label;
            if (this.settings?.input_range === qr.value) {
                option.selected = true;
            }
            select.appendChild(option);
        }

        select.addEventListener('change', (e) => {
            this.updateSetting('input_range', e.target.value);
        });

        wrapper.appendChild(select);

        const help = document.createElement('div');
        help.className = 'color-control-help';
        help.textContent = 'Limited for TV/broadcast, Full for PC/gaming';
        wrapper.appendChild(help);

        return wrapper;
    }

    /**
     * Create a slider control
     */
    createSlider(id, label, min, max, step, defaultValue) {
        const wrapper = document.createElement('div');
        wrapper.className = 'color-control color-slider-control';

        const header = document.createElement('div');
        header.className = 'color-slider-header';

        const labelEl = document.createElement('label');
        labelEl.textContent = label;
        labelEl.className = 'color-control-label';
        header.appendChild(labelEl);

        const valueEl = document.createElement('span');
        valueEl.className = 'color-slider-value';
        valueEl.id = `color-value-${id}`;
        valueEl.textContent = this.formatValue(id, this.settings?.[id] ?? defaultValue);
        header.appendChild(valueEl);

        wrapper.appendChild(header);

        const slider = document.createElement('input');
        slider.type = 'range';
        slider.className = 'color-slider';
        slider.id = `color-slider-${id}`;
        slider.min = min;
        slider.max = max;
        slider.step = step;
        slider.value = this.settings?.[id] ?? defaultValue;
        slider.dataset.default = defaultValue;
        slider.dataset.settingId = id;

        // Snap threshold (2% of range)
        const range = max - min;
        const snapThreshold = range * 0.02;

        slider.addEventListener('input', (e) => {
            let value = parseFloat(e.target.value);
            const defaultVal = parseFloat(e.target.dataset.default);

            // Snap to default if close
            if (Math.abs(value - defaultVal) <= snapThreshold) {
                value = defaultVal;
                e.target.value = value;
            }

            valueEl.textContent = this.formatValue(id, value);
            valueEl.classList.toggle('at-default', value === defaultVal);
        });

        slider.addEventListener('change', (e) => {
            const value = parseFloat(e.target.value);
            this.updateSetting(id, value);
        });

        wrapper.appendChild(slider);

        const meta = document.createElement('div');
        meta.className = 'color-slider-meta';
        meta.innerHTML = `<span>${min}</span><span>Default: ${defaultValue}</span><span>${max}</span>`;
        wrapper.appendChild(meta);

        return wrapper;
    }

    /**
     * Format a value for display
     */
    formatValue(id, value) {
        if (id === 'brightness' || id === 'hue') {
            return value > 0 ? `+${value}` : `${value}`;
        }
        if (id === 'contrast' || id === 'saturation' || id === 'gamma') {
            return value.toFixed(2);
        }
        return value;
    }

    /**
     * Update control states based on enabled status
     */
    updateControlStates() {
        const enabled = this.settings?.enabled || false;
        const controls = document.querySelectorAll('.color-slider, .color-select, .color-preset-btn');

        controls.forEach(ctrl => {
            if (ctrl.classList.contains('color-preset-btn')) {
                // Presets should always be clickable
                return;
            }
            ctrl.disabled = !enabled;
            ctrl.closest('.color-control')?.classList.toggle('disabled', !enabled);
        });
    }

    /**
     * Update a single setting
     */
    async updateSetting(key, value) {
        try {
            const body = {};
            body[key] = value;

            const response = await fetch('/api/color', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body)
            });

            if (!response.ok) {
                throw new Error('Failed to update setting');
            }

            // Update local state
            if (this.settings) {
                this.settings[key] = value;
            }
        } catch (error) {
            console.error('Failed to update color setting:', error);
            showToast('Failed to update color setting', 'error');
        }
    }

    /**
     * Create auto white balance button
     */
    createAutoWBButton() {
        const wrapper = document.createElement('div');
        wrapper.className = 'color-auto-wb-wrapper';
        wrapper.style.marginBottom = '12px';

        const btn = document.createElement('button');
        btn.className = 'color-auto-wb-btn';
        btn.innerHTML = '⚖️ Auto White Balance';
        btn.style.cssText = 'width: 100%; padding: 8px 12px; background: #4a5568; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 14px;';

        btn.addEventListener('click', async () => {
            btn.disabled = true;
            btn.innerHTML = '⏳ Calculating...';

            try {
                const response = await fetch('/api/color/auto-white-balance', {
                    method: 'POST'
                });

                const result = await response.json();

                if (result.success) {
                    // Reload settings to update sliders
                    await this.load();
                    showToast(`White balance applied (${result.message})`, 'success');
                } else {
                    showToast(result.message || 'Failed to calculate white balance', 'error');
                }
            } catch (error) {
                console.error('Auto white balance failed:', error);
                showToast('Auto white balance failed', 'error');
            } finally {
                btn.disabled = false;
                btn.innerHTML = '⚖️ Auto White Balance';
            }
        });

        wrapper.appendChild(btn);

        const hint = document.createElement('div');
        hint.style.cssText = 'font-size: 11px; color: #888; margin-top: 4px;';
        hint.textContent = 'Best with neutral/gray content on screen';
        wrapper.appendChild(hint);

        return wrapper;
    }

    /**
     * Apply a preset
     */
    async applyPreset(presetId) {
        try {
            const response = await fetch(`/api/color/preset/${presetId}`, {
                method: 'POST'
            });

            if (!response.ok) {
                throw new Error('Failed to apply preset');
            }

            // Reload settings to reflect the preset
            await this.load();

            const preset = this.presets.find(p => p.id === presetId);
            showToast(`Applied: ${preset?.name || presetId}`, 'success');
        } catch (error) {
            console.error('Failed to apply color preset:', error);
            showToast('Failed to apply color preset', 'error');
        }
    }
}
