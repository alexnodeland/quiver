// Initialize Mermaid with theme-aware configuration
document.addEventListener('DOMContentLoaded', function() {
    // Detect current theme
    const isDark = document.documentElement.classList.contains('ayu') ||
                   document.documentElement.classList.contains('coal') ||
                   document.documentElement.classList.contains('navy');

    mermaid.initialize({
        startOnLoad: true,
        theme: isDark ? 'dark' : 'default',
        themeVariables: {
            primaryColor: '#4a9eff',
            primaryTextColor: '#fff',
            primaryBorderColor: '#3a8eef',
            lineColor: '#f9a826',
            secondaryColor: '#50c878',
            tertiaryColor: '#f0f0f0',
            noteTextColor: '#333',
            noteBkgColor: '#fff5ad',
        },
        flowchart: {
            useMaxWidth: true,
            htmlLabels: true,
            curve: 'basis'
        },
        sequence: {
            useMaxWidth: true,
            mirrorActors: false
        }
    });

    // Re-render on theme change
    const observer = new MutationObserver(function(mutations) {
        mutations.forEach(function(mutation) {
            if (mutation.attributeName === 'class') {
                location.reload();
            }
        });
    });

    observer.observe(document.documentElement, { attributes: true });
});
