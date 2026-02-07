/**
 * PiSovereign Documentation - Version Switcher
 * 
 * This script adds a version selector dropdown to the mdBook navigation.
 * It fetches available versions from versions.json and allows users to
 * switch between different documentation versions.
 */

(function() {
    'use strict';

    // Configuration
    const CONFIG = {
        versionsUrl: '/PiSovereign/versions.json',
        currentVersionMeta: 'pisovereign-version',
        storageKey: 'pisovereign-docs-version',
        fallbackVersions: [
            { version: 'latest', label: 'Latest', type: 'latest' },
            { version: 'main', label: 'Development', type: 'dev' }
        ]
    };

    /**
     * Get current version from meta tag or URL path
     */
    function getCurrentVersion() {
        // Try meta tag first
        const meta = document.querySelector(`meta[name="${CONFIG.currentVersionMeta}"]`);
        if (meta) {
            return meta.getAttribute('content');
        }

        // Parse from URL path: /PiSovereign/v1.0.0/... or /PiSovereign/main/...
        const pathMatch = window.location.pathname.match(/\/PiSovereign\/([^/]+)\//);
        if (pathMatch) {
            return pathMatch[1];
        }

        return 'latest';
    }

    /**
     * Fetch available versions from versions.json
     */
    async function fetchVersions() {
        try {
            const response = await fetch(CONFIG.versionsUrl);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            return await response.json();
        } catch (error) {
            console.warn('Failed to fetch versions.json, using fallback:', error);
            return { versions: CONFIG.fallbackVersions };
        }
    }

    /**
     * Create version selector dropdown
     */
    function createVersionSelector(versions, currentVersion) {
        const container = document.createElement('div');
        container.className = 'version-selector';

        const select = document.createElement('select');
        select.setAttribute('aria-label', 'Documentation version');
        select.id = 'version-select';

        versions.forEach(v => {
            const option = document.createElement('option');
            option.value = v.version;
            option.textContent = v.label || v.version;
            
            if (v.version === currentVersion) {
                option.selected = true;
            }

            // Add visual indicator for version type
            if (v.type === 'latest') {
                option.textContent += ' ✓';
            } else if (v.type === 'dev') {
                option.textContent += ' (dev)';
            }

            select.appendChild(option);
        });

        select.addEventListener('change', function() {
            const newVersion = this.value;
            navigateToVersion(newVersion, currentVersion);
        });

        container.appendChild(select);
        return container;
    }

    /**
     * Navigate to a different documentation version
     */
    function navigateToVersion(newVersion, currentVersion) {
        let currentPath = window.location.pathname;
        
        // Replace version in path
        const versionPattern = new RegExp(`/PiSovereign/${currentVersion}/`);
        let newPath;

        if (versionPattern.test(currentPath)) {
            newPath = currentPath.replace(versionPattern, `/PiSovereign/${newVersion}/`);
        } else {
            // Fallback: go to version root
            newPath = `/PiSovereign/${newVersion}/`;
        }

        // Store preference
        try {
            localStorage.setItem(CONFIG.storageKey, newVersion);
        } catch (e) {
            // localStorage may be unavailable
        }

        window.location.href = newPath;
    }

    /**
     * Show warning banner for old versions
     */
    function showVersionWarning(versions, currentVersion) {
        const versionInfo = versions.find(v => v.version === currentVersion);
        const mainContent = document.querySelector('.content main');
        
        if (!mainContent) return;

        let banner = null;

        if (versionInfo && versionInfo.type === 'old') {
            banner = document.createElement('div');
            banner.className = 'version-warning';
            banner.innerHTML = `
                <strong>⚠️ Outdated Documentation</strong><br>
                You are viewing documentation for version <code>${currentVersion}</code>.
                <a href="/PiSovereign/latest/">View the latest version</a>.
            `;
        } else if (currentVersion === 'main') {
            banner = document.createElement('div');
            banner.className = 'dev-notice';
            banner.innerHTML = `
                <strong>📝 Development Documentation</strong><br>
                This documentation is for the development branch and may include unreleased features.
                <a href="/PiSovereign/latest/">View the stable release documentation</a>.
            `;
        }

        if (banner) {
            mainContent.insertBefore(banner, mainContent.firstChild);
        }
    }

    /**
     * Insert version selector into the navigation bar
     */
    function insertIntoNavbar(selector) {
        // Try to find the right-buttons container in mdBook
        const rightButtons = document.querySelector('.right-buttons');
        
        if (rightButtons) {
            // Insert before the first button
            rightButtons.insertBefore(selector, rightButtons.firstChild);
        } else {
            // Fallback: append to menu bar
            const menuBar = document.querySelector('.menu-bar');
            if (menuBar) {
                menuBar.appendChild(selector);
            }
        }
    }

    /**
     * Initialize version switcher
     */
    async function init() {
        // Add CSS
        const cssLink = document.createElement('link');
        cssLink.rel = 'stylesheet';
        cssLink.href = '/PiSovereign/theme/css/version-switcher.css';
        document.head.appendChild(cssLink);

        // Get current version
        const currentVersion = getCurrentVersion();

        // Fetch available versions
        const data = await fetchVersions();
        const versions = data.versions || CONFIG.fallbackVersions;

        // Create and insert selector
        const selector = createVersionSelector(versions, currentVersion);
        insertIntoNavbar(selector);

        // Show warning if viewing old version
        showVersionWarning(versions, currentVersion);

        // Log for debugging
        console.log(`PiSovereign Docs: Version ${currentVersion}, ${versions.length} versions available`);
    }

    // Run when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
