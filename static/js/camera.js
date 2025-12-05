/**
 * HyperCalibrate - Camera Controls Module
 * Handles camera control panel with sliders, toggles, and menus
 */

import { showToast, formatControlName, normalizeControlName } from './utils.js';

export class CameraManager {
    constructor(app) {
        this.app = app;
        this.controls = [];
        this.panelVisible = false;
        this.panelElement = null;

        // Control dependencies (e.g., white_balance_temperature depends on auto_white_balance being off)
        this.controlDependencies = {
            'white_balance_temperature': { dependsOn: 'white_balance_automatic', activeWhen: false },
            'exposure_time_absolute': { dependsOn: 'auto_exposure', activeWhen: false },
            'focus_absolute': { dependsOn: 'focus_automatic_continuous', activeWhen: false },
            'gain': { dependsOn: 'autogain', activeWhen: false }
        };
    }

    /**
     * Initialize camera manager
     */
    init(panelElement) {
        this.panelElement = panelElement;
    }

    /**
     * Toggle the camera panel visibility
     */
    toggle(show) {
        if (show === undefined) {
            show = !this.panelVisible;
        }
        this.panelVisible = show;

        if (show) {
            this.panelElement?.classList.remove('hidden');
            document.body.classList.add('panel-open');
            this.load();
        } else {
            this.panelElement?.classList.add('hidden');
            document.body.classList.remove('panel-open');
        }

        return this.panelVisible;
    }

    /**
     * Load camera controls from server
     */
    async load() {
        const loadingEl = document.getElementById('camera-controls-loading');
        const containerEl = document.getElementById('camera-controls-container');
        const unavailableEl = document.getElementById('camera-controls-unavailable');

        loadingEl?.classList.remove('hidden');
        containerEl?.classList.add('hidden');
        unavailableEl?.classList.add('hidden');

        try {
            const response = await fetch('/api/camera/controls');
            const data = await response.json();

            loadingEl?.classList.add('hidden');

            if (!data.available || data.controls.length === 0) {
                unavailableEl?.classList.remove('hidden');
                return;
            }

            this.controls = data.controls;
            this.render();
            containerEl?.classList.remove('hidden');
        } catch (error) {
            console.error('Failed to load camera controls:', error);
            loadingEl?.classList.add('hidden');
            unavailableEl?.classList.remove('hidden');
        }
    }

    /**
     * Render camera controls
     */
    render() {
        const container = document.getElementById('camera-controls-container');
        if (!container) return;

        container.innerHTML = '';

        // Group controls by category
        const userControls = [];
        const cameraControls = [];

        // Control groups - toggle rendered above slider
        const controlGroups = {
            'white_balance_temperature': 'white_balance_automatic',
            'exposure_time_absolute': 'auto_exposure',
            'focus_absolute': 'focus_automatic_continuous',
            'gain': 'autogain'
        };

        const groupedControls = new Set();

        for (const control of this.controls) {
            if (control.flags.disabled) continue;

            const controlName = normalizeControlName(control.name);

            if (Object.values(controlGroups).includes(controlName)) {
                groupedControls.add(controlName);
            }

            // Camera class controls have IDs starting with 0x009a
            if (control.id >= 0x009a0000 && control.id < 0x009b0000) {
                cameraControls.push(control);
            } else {
                userControls.push(control);
            }
        }

        if (userControls.length > 0) {
            const category = this.createCategory('Image Controls', userControls, controlGroups, groupedControls);
            container.appendChild(category);
        }

        if (cameraControls.length > 0) {
            const category = this.createCategory('Camera Controls', cameraControls, controlGroups, groupedControls);
            container.appendChild(category);
        }
    }

    /**
     * Create a control category element
     */
    createCategory(title, controls, controlGroups, groupedControls) {
        const categoryEl = document.createElement('div');
        categoryEl.className = 'control-category';

        const titleEl = document.createElement('div');
        titleEl.className = 'control-category-title';
        titleEl.textContent = title;
        categoryEl.appendChild(titleEl);

        for (const control of controls) {
            const controlName = normalizeControlName(control.name);

            if (groupedControls.has(controlName)) {
                continue;
            }

            const groupedToggleName = controlGroups[controlName];
            let groupedToggle = null;
            if (groupedToggleName) {
                groupedToggle = this.controls.find(
                    c => normalizeControlName(c.name) === groupedToggleName
                );
            }

            const controlEl = this.createControlElement(control, groupedToggle);
            categoryEl.appendChild(controlEl);
        }

        return categoryEl;
    }

    /**
     * Create a control element (slider, toggle, or menu)
     */
    createControlElement(control, groupedToggle = null) {
        const el = document.createElement('div');
        el.className = 'camera-control';
        el.dataset.controlId = control.id;

        if (control.flags.inactive) {
            el.classList.add('inactive');
        }

        // Render grouped toggle first
        if (groupedToggle) {
            const toggleHeader = document.createElement('div');
            toggleHeader.className = 'camera-control-header';

            const toggleNameEl = document.createElement('span');
            toggleNameEl.className = 'camera-control-name';
            toggleNameEl.textContent = formatControlName(groupedToggle.name);
            toggleHeader.appendChild(toggleNameEl);

            const toggleValueEl = document.createElement('span');
            toggleValueEl.className = 'camera-control-value';
            toggleValueEl.id = 'camera-value-' + groupedToggle.id;
            toggleHeader.appendChild(toggleValueEl);

            el.appendChild(toggleHeader);
            el.appendChild(this.createBooleanControl(groupedToggle, toggleValueEl));

            const separator = document.createElement('div');
            separator.className = 'control-group-separator';
            el.appendChild(separator);
        }

        const header = document.createElement('div');
        header.className = 'camera-control-header';

        const nameEl = document.createElement('span');
        nameEl.className = 'camera-control-name';
        nameEl.textContent = formatControlName(control.name);
        header.appendChild(nameEl);

        const valueEl = document.createElement('span');
        valueEl.className = 'camera-control-value';
        valueEl.id = 'camera-value-' + control.id;
        header.appendChild(valueEl);

        el.appendChild(header);

        switch (control.type) {
            case 'boolean':
                el.appendChild(this.createBooleanControl(control, valueEl));
                break;
            case 'menu':
            case 'integermenu':
                el.appendChild(this.createMenuControl(control, valueEl));
                break;
            default:
                el.appendChild(this.createSliderControl(control, valueEl));
                break;
        }

        return el;
    }

    /**
     * Create a slider control
     */
    createSliderControl(control, valueEl) {
        const wrapper = document.createElement('div');
        const controlName = normalizeControlName(control.name);

        const slider = document.createElement('input');
        slider.type = 'range';
        slider.className = 'camera-control-slider';
        slider.min = control.minimum;
        slider.max = control.maximum;
        slider.step = control.step || 1;
        slider.value = this.getControlValue(control);
        slider.dataset.controlName = controlName;
        slider.dataset.defaultValue = control.default;

        // Calculate snap threshold (2% of range, minimum of 1 step)
        const range = control.maximum - control.minimum;
        const snapThreshold = Math.max(range * 0.02, control.step || 1);
        slider.dataset.snapThreshold = snapThreshold;

        // Check dependencies
        const dependency = this.controlDependencies[controlName];
        if (dependency) {
            const parentControl = this.controls.find(
                c => normalizeControlName(c.name) === dependency.dependsOn
            );
            if (parentControl) {
                const parentValue = this.getControlValue(parentControl);
                const parentBoolValue = parentValue === true || parentValue === 1;
                slider.disabled = (parentBoolValue !== dependency.activeWhen);
            }
        }

        if (control.flags.inactive) {
            slider.disabled = true;
        }

        const isAtDefault = parseInt(slider.value) === control.default;
        valueEl.textContent = slider.value;
        if (slider.disabled) {
            valueEl.textContent += ' (inactive)';
        } else if (isAtDefault) {
            valueEl.classList.add('at-default');
        }

        slider.addEventListener('input', (e) => {
            let value = parseInt(e.target.value);
            const defaultVal = parseInt(e.target.dataset.defaultValue);
            const threshold = parseFloat(e.target.dataset.snapThreshold);

            // Snap to default if within threshold
            if (Math.abs(value - defaultVal) <= threshold) {
                value = defaultVal;
                e.target.value = value;
                valueEl.classList.add('at-default');
            } else {
                valueEl.classList.remove('at-default');
            }

            valueEl.textContent = value + (e.target.disabled ? ' (inactive)' : '');
        });

        slider.addEventListener('change', (e) => {
            this.setControl(control.id, parseInt(e.target.value));
        });

        wrapper.appendChild(slider);

        const metaEl = document.createElement('div');
        metaEl.className = 'camera-control-meta';
        metaEl.innerHTML = `<span>${control.minimum}</span><span>Default: ${control.default}</span><span>${control.maximum}</span>`;
        wrapper.appendChild(metaEl);

        return wrapper;
    }

    /**
     * Create a boolean toggle control
     */
    createBooleanControl(control, valueEl) {
        const wrapper = document.createElement('div');
        wrapper.className = 'camera-control-toggle';

        const toggle = document.createElement('label');
        toggle.className = 'toggle';

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = this.getControlValue(control) === true || this.getControlValue(control) === 1;
        checkbox.dataset.controlId = control.id;
        checkbox.dataset.controlName = normalizeControlName(control.name);

        valueEl.textContent = checkbox.checked ? 'On' : 'Off';

        checkbox.addEventListener('change', async (e) => {
            valueEl.textContent = e.target.checked ? 'On' : 'Off';
            const controlName = normalizeControlName(control.name);
            this.updateDependentControls(controlName, e.target.checked);
            this.setControl(control.id, e.target.checked);
        });

        const slider = document.createElement('span');
        slider.className = 'toggle-slider';

        toggle.appendChild(checkbox);
        toggle.appendChild(slider);
        wrapper.appendChild(toggle);

        return wrapper;
    }

    /**
     * Create a menu select control
     */
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
            this.setControl(control.id, parseInt(e.target.value));
        });

        return select;
    }

    /**
     * Get control value handling different value types
     */
    getControlValue(control) {
        if (control.value === null || control.value === undefined) {
            return control.default;
        }
        if (typeof control.value === 'object') {
            if ('Integer' in control.value) return control.value.Integer;
            if ('Boolean' in control.value) return control.value.Boolean;
            if ('String' in control.value) return control.value.String;
        }
        return control.value;
    }

    /**
     * Update dependent controls based on parent toggle
     */
    updateDependentControls(controlName, value) {
        for (const [dependentName, dependency] of Object.entries(this.controlDependencies)) {
            if (dependency.dependsOn === controlName) {
                const shouldBeActive = (value === dependency.activeWhen);
                const slider = document.querySelector(`.camera-control-slider[data-control-name="${dependentName}"]`);

                if (slider) {
                    const controlEl = slider.closest('.camera-control');
                    slider.disabled = !shouldBeActive;
                    controlEl?.classList.toggle('inactive', !shouldBeActive);

                    const valueEl = controlEl?.querySelectorAll('.camera-control-value');
                    const lastValueEl = valueEl?.[valueEl.length - 1];
                    if (lastValueEl) {
                        lastValueEl.textContent = slider.value + (!shouldBeActive ? ' (inactive)' : '');
                    }
                }
            }
        }
    }

    /**
     * Set a camera control value
     */
    async setControl(id, value) {
        try {
            const response = await fetch('/api/camera/control/' + id, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ value: value })
            });

            if (!response.ok) {
                const errorData = await response.json().catch(() => ({}));
                throw new Error(errorData.error || 'Failed to set control');
            }

            setTimeout(() => this.refreshStates(), 200);
        } catch (error) {
            console.error('Failed to set camera control:', error);
            showToast('Failed to set camera control: ' + error.message, 'error');
        }
    }

    /**
     * Refresh control states from server
     */
    async refreshStates() {
        try {
            const response = await fetch('/api/camera/controls');
            const data = await response.json();

            if (!data.available || !data.controls) return;

            this.controls = data.controls;

            for (const control of data.controls) {
                const controlName = normalizeControlName(control.name);
                const slider = document.querySelector(`.camera-control-slider[data-control-name="${controlName}"]`);
                const controlEl = slider?.closest('.camera-control');

                if (controlEl) {
                    this.updateControlState(controlEl, control, controlName);
                }
            }
        } catch (error) {
            console.error('Failed to refresh camera control states:', error);
        }
    }

    /**
     * Update a single control's state
     */
    updateControlState(el, control, controlName) {
        const slider = el.querySelector(`.camera-control-slider[data-control-name="${controlName}"]`) ||
                       el.querySelector('.camera-control-slider');
        const select = el.querySelector('.camera-control-select');
        const input = slider || select;

        if (!input) return;

        const dependency = this.controlDependencies[controlName];
        let shouldBeDisabled = false;

        if (dependency) {
            const parentControl = this.controls.find(
                c => normalizeControlName(c.name) === dependency.dependsOn
            );
            if (parentControl) {
                const parentValue = this.getControlValue(parentControl);
                const parentBoolValue = parentValue === true || parentValue === 1;
                shouldBeDisabled = (parentBoolValue !== dependency.activeWhen);
            }
        } else {
            shouldBeDisabled = control.flags.inactive;
        }

        input.disabled = shouldBeDisabled;
        el.classList.toggle('inactive', shouldBeDisabled);

        const valueEl = el.querySelector(`#camera-value-${control.id}`) ||
                       el.querySelectorAll('.camera-control-value')[el.querySelectorAll('.camera-control-header').length - 1];

        if (valueEl && slider) {
            const currentValue = this.getControlValue(control);
            slider.value = currentValue;
            valueEl.textContent = shouldBeDisabled ? currentValue + ' (inactive)' : currentValue;
        }
    }

    /**
     * Reset all camera controls to defaults
     */
    async reset() {
        try {
            const response = await fetch('/api/camera/controls/reset', { method: 'POST' });
            if (response.ok) {
                await this.load();
                showToast('Camera controls reset', 'success');
            } else {
                throw new Error('Reset failed');
            }
        } catch (error) {
            console.error('Failed to reset camera controls:', error);
            showToast('Failed to reset camera controls', 'error');
        }
    }

    /**
     * Refresh camera controls from hardware
     */
    async refresh() {
        try {
            await fetch('/api/camera/controls/refresh', { method: 'POST' });
            await this.load();
            showToast('Camera controls refreshed', 'success');
        } catch (error) {
            console.error('Failed to refresh camera controls:', error);
            showToast('Failed to refresh camera controls', 'error');
        }
    }
}
