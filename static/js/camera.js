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
        // For menu-type controls like auto_exposure, use 'activeWhenValue' to specify the exact value(s)
        // that enable the dependent control (e.g., auto_exposure=1 means Manual Mode)
        this.controlDependencies = {
            'white_balance_temperature': { dependsOn: 'white_balance_automatic', activeWhen: false },
            'exposure_time_absolute': { dependsOn: 'auto_exposure', activeWhenValues: [1] }, // 1 = Manual Mode
            'focus_absolute': { dependsOn: 'focus_automatic_continuous', activeWhen: false },
            'gain': { dependsOn: 'autogain', activeWhen: false }
        };

        // Control descriptions for tooltips/help
        this.controlDescriptions = {
            // Brightness & Exposure
            'brightness': 'Adjusts overall image brightness. Higher values make the image lighter, lower values make it darker.',
            'contrast': 'Controls the difference between light and dark areas. Higher contrast makes colors more vivid but may lose detail in shadows/highlights.',
            'saturation': 'Adjusts color intensity. Higher values make colors more vivid, lower values move toward grayscale.',
            'hue': 'Shifts all colors around the color wheel. Useful for correcting color casts.',
            'gamma': 'Adjusts midtone brightness without affecting pure blacks or whites. Higher gamma lightens midtones.',
            'sharpness': 'Enhances edge definition. Too high can introduce artifacts; too low makes the image soft.',
            'backlight_compensation': 'Helps expose subjects properly when there\'s bright light behind them.',

            // Exposure controls
            'exposure_time_absolute': 'How long the sensor captures light per frame (in 100µs units). Longer = brighter but more motion blur.',
            'auto_exposure': 'Lets the camera automatically adjust exposure. Manual mode gives you direct control.',
            'exposure_dynamic_framerate': 'Allows the camera to reduce framerate in low light for better exposure.',
            'gain': 'Amplifies the sensor signal. Higher gain = brighter image but more noise/grain.',
            'autogain': 'Automatically adjusts gain based on scene brightness.',

            // White Balance
            'white_balance_temperature': 'Color temperature in Kelvin. Lower (2000-4000K) = warmer/orange, Higher (5500-8000K) = cooler/blue.',
            'white_balance_automatic': 'Automatically adjusts white balance to neutralize color casts from different light sources.',

            // Focus
            'focus_absolute': 'Manual focus distance. Lower values focus closer, higher values focus farther away.',
            'focus_automatic_continuous': 'Continuously adjusts focus to keep subjects sharp. Disable for manual focus control.',

            // Pan/Tilt/Zoom
            'pan_absolute': 'Moves the camera view horizontally (left/right). Useful for framing without moving the camera.',
            'tilt_absolute': 'Moves the camera view vertically (up/down). Useful for framing without moving the camera.',
            'zoom_absolute': 'Digital or optical zoom level. Higher values zoom in, showing a smaller area with more detail.',

            // Power line frequency
            'power_line_frequency': 'Matches your local power frequency (50Hz/60Hz) to prevent flickering under artificial lights.',

            // Other common controls
            'led1_mode': 'Controls the camera\'s LED indicator behavior.',
            'led1_frequency': 'Sets the blink rate of the camera LED when in blinking mode.'
        };
    }

    /**
     * Initialize camera manager
     */
    init(panelElement) {
        this.panelElement = panelElement;

        // Close descriptions when clicking outside
        document.addEventListener('click', (e) => {
            if (!e.target.closest('.control-info-btn') && !e.target.closest('.control-description')) {
                this.closeAllDescriptions();
            }
        });
    }

    /**
     * Close all open control descriptions
     */
    closeAllDescriptions() {
        document.querySelectorAll('.control-description.show').forEach(d => {
            d.classList.remove('show');
            d.remove();
        });
        document.querySelectorAll('.control-info-btn.active').forEach(b => b.classList.remove('active'));
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

        // Render grouped toggle/menu first (e.g., auto_exposure above exposure_time)
        if (groupedToggle) {
            const toggleHeader = document.createElement('div');
            toggleHeader.className = 'camera-control-header';

            const toggleNameWrapper = this.createNameWithInfo(groupedToggle);
            toggleHeader.appendChild(toggleNameWrapper);

            const toggleValueEl = document.createElement('span');
            toggleValueEl.className = 'camera-control-value';
            toggleValueEl.id = 'camera-value-' + groupedToggle.id;
            toggleHeader.appendChild(toggleValueEl);

            el.appendChild(toggleHeader);
            
            // Render based on actual control type (menu vs boolean)
            if (groupedToggle.type === 'menu' || groupedToggle.type === 'integermenu') {
                el.appendChild(this.createMenuControl(groupedToggle, toggleValueEl));
            } else {
                el.appendChild(this.createBooleanControl(groupedToggle, toggleValueEl));
            }

            const separator = document.createElement('div');
            separator.className = 'control-group-separator';
            el.appendChild(separator);
        }

        const header = document.createElement('div');
        header.className = 'camera-control-header';

        const nameWrapper = this.createNameWithInfo(control);
        header.appendChild(nameWrapper);

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
     * Create control name with info button
     */
    createNameWithInfo(control) {
        const wrapper = document.createElement('div');
        wrapper.className = 'camera-control-name-wrapper';

        const nameEl = document.createElement('span');
        nameEl.className = 'camera-control-name';
        nameEl.textContent = formatControlName(control.name);
        wrapper.appendChild(nameEl);

        const controlName = normalizeControlName(control.name);
        const description = this.controlDescriptions[controlName];

        if (description) {
            const infoBtn = document.createElement('button');
            infoBtn.className = 'control-info-btn';
            infoBtn.innerHTML = 'ⓘ';
            infoBtn.type = 'button';
            infoBtn.setAttribute('aria-label', 'Show help for ' + formatControlName(control.name));
            infoBtn.dataset.description = description;

            infoBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                this.toggleDescription(infoBtn);
            });

            wrapper.appendChild(infoBtn);
        }

        return wrapper;
    }

    /**
     * Toggle description panel for a control
     */
    toggleDescription(infoBtn) {
        const controlEl = infoBtn.closest('.camera-control');
        let descPanel = controlEl.querySelector('.control-description');

        // Close any other open descriptions first
        document.querySelectorAll('.control-description.show').forEach(d => {
            if (d !== descPanel) {
                d.classList.remove('show');
                d.remove();
            }
        });
        document.querySelectorAll('.control-info-btn.active').forEach(b => {
            if (b !== infoBtn) b.classList.remove('active');
        });

        if (descPanel) {
            // Already exists, toggle it
            descPanel.classList.toggle('show');
            infoBtn.classList.toggle('active');
            if (!descPanel.classList.contains('show')) {
                descPanel.remove();
            }
        } else {
            // Create and show
            descPanel = document.createElement('div');
            descPanel.className = 'control-description show';
            descPanel.textContent = infoBtn.dataset.description;
            controlEl.appendChild(descPanel);
            infoBtn.classList.add('active');
        }
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
                slider.disabled = !this.isDependencyMet(dependency, this.getControlValue(parentControl));
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
        select.dataset.controlName = normalizeControlName(control.name);

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
            const newValue = parseInt(e.target.value);
            valueEl.textContent = selectedOpt ? selectedOpt.textContent : newValue;
            const controlName = normalizeControlName(control.name);
            this.updateDependentControls(controlName, newValue);
            this.setControl(control.id, newValue);
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
     * Check if a dependency condition is met
     * Supports both boolean-style (activeWhen: true/false) and menu-style (activeWhenValues: [1, 2])
     */
    isDependencyMet(dependency, parentValue) {
        // For menu-type controls with specific active values (e.g., auto_exposure = 1 for manual)
        if (dependency.activeWhenValues !== undefined) {
            return dependency.activeWhenValues.includes(parentValue);
        }
        // For boolean-type controls
        const parentBoolValue = parentValue === true || parentValue === 1;
        return parentBoolValue === dependency.activeWhen;
    }

    /**
     * Update dependent controls based on parent toggle
     */
    updateDependentControls(controlName, value) {
        for (const [dependentName, dependency] of Object.entries(this.controlDependencies)) {
            if (dependency.dependsOn === controlName) {
                const shouldBeActive = this.isDependencyMet(dependency, value);
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
                shouldBeDisabled = !this.isDependencyMet(dependency, this.getControlValue(parentControl));
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
