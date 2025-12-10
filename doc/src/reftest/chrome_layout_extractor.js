// Chrome Layout Extractor Script
// This script extracts detailed layout information from a rendered page
// for comparison with Azul's layout engine output.

(function() {
    const result = {
        timestamp: new Date().toISOString(),
        viewport: {
            width: window.innerWidth,
            height: window.innerHeight,
            devicePixelRatio: window.devicePixelRatio
        },
        elements: [],
        computedStyles: [],
        textMetrics: [],
        fonts: []
    };

    // Get all elements in the document
    const allElements = document.querySelectorAll('*');
    
    allElements.forEach((el, index) => {
        // Skip script and style elements
        if (el.tagName === 'SCRIPT' || el.tagName === 'STYLE' || el.tagName === 'HEAD') {
            return;
        }

        const rect = el.getBoundingClientRect();
        const computedStyle = window.getComputedStyle(el);
        
        // Basic element info
        const elementInfo = {
            index: index,
            tagName: el.tagName.toLowerCase(),
            id: el.id || null,
            className: el.className || null,
            // Bounding box
            bounds: {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                left: rect.left
            },
            // Box model
            boxModel: {
                marginTop: parseFloat(computedStyle.marginTop) || 0,
                marginRight: parseFloat(computedStyle.marginRight) || 0,
                marginBottom: parseFloat(computedStyle.marginBottom) || 0,
                marginLeft: parseFloat(computedStyle.marginLeft) || 0,
                paddingTop: parseFloat(computedStyle.paddingTop) || 0,
                paddingRight: parseFloat(computedStyle.paddingRight) || 0,
                paddingBottom: parseFloat(computedStyle.paddingBottom) || 0,
                paddingLeft: parseFloat(computedStyle.paddingLeft) || 0,
                borderTopWidth: parseFloat(computedStyle.borderTopWidth) || 0,
                borderRightWidth: parseFloat(computedStyle.borderRightWidth) || 0,
                borderBottomWidth: parseFloat(computedStyle.borderBottomWidth) || 0,
                borderLeftWidth: parseFloat(computedStyle.borderLeftWidth) || 0
            },
            // Layout properties
            layout: {
                display: computedStyle.display,
                position: computedStyle.position,
                float: computedStyle.float,
                clear: computedStyle.clear,
                overflow: computedStyle.overflow,
                overflowX: computedStyle.overflowX,
                overflowY: computedStyle.overflowY,
                boxSizing: computedStyle.boxSizing,
                zIndex: computedStyle.zIndex
            },
            // Flexbox properties (if applicable)
            flexbox: computedStyle.display.includes('flex') ? {
                flexDirection: computedStyle.flexDirection,
                flexWrap: computedStyle.flexWrap,
                justifyContent: computedStyle.justifyContent,
                alignItems: computedStyle.alignItems,
                alignContent: computedStyle.alignContent,
                flexGrow: computedStyle.flexGrow,
                flexShrink: computedStyle.flexShrink,
                flexBasis: computedStyle.flexBasis,
                alignSelf: computedStyle.alignSelf,
                order: computedStyle.order
            } : null,
            // Grid properties (if applicable)
            grid: computedStyle.display.includes('grid') ? {
                gridTemplateColumns: computedStyle.gridTemplateColumns,
                gridTemplateRows: computedStyle.gridTemplateRows,
                gridTemplateAreas: computedStyle.gridTemplateAreas,
                gridAutoColumns: computedStyle.gridAutoColumns,
                gridAutoRows: computedStyle.gridAutoRows,
                gridAutoFlow: computedStyle.gridAutoFlow,
                gridGap: computedStyle.gap || computedStyle.gridGap,
                gridColumnStart: computedStyle.gridColumnStart,
                gridColumnEnd: computedStyle.gridColumnEnd,
                gridRowStart: computedStyle.gridRowStart,
                gridRowEnd: computedStyle.gridRowEnd
            } : null,
            // Size constraints
            sizing: {
                width: computedStyle.width,
                height: computedStyle.height,
                minWidth: computedStyle.minWidth,
                minHeight: computedStyle.minHeight,
                maxWidth: computedStyle.maxWidth,
                maxHeight: computedStyle.maxHeight
            },
            // Positioning
            positioning: {
                top: computedStyle.top,
                right: computedStyle.right,
                bottom: computedStyle.bottom,
                left: computedStyle.left
            },
            // Typography
            typography: {
                fontFamily: computedStyle.fontFamily,
                fontSize: computedStyle.fontSize,
                fontWeight: computedStyle.fontWeight,
                fontStyle: computedStyle.fontStyle,
                lineHeight: computedStyle.lineHeight,
                letterSpacing: computedStyle.letterSpacing,
                textAlign: computedStyle.textAlign,
                textDecoration: computedStyle.textDecoration,
                whiteSpace: computedStyle.whiteSpace,
                wordBreak: computedStyle.wordBreak,
                wordSpacing: computedStyle.wordSpacing
            },
            // Colors
            colors: {
                color: computedStyle.color,
                backgroundColor: computedStyle.backgroundColor,
                borderTopColor: computedStyle.borderTopColor,
                borderRightColor: computedStyle.borderRightColor,
                borderBottomColor: computedStyle.borderBottomColor,
                borderLeftColor: computedStyle.borderLeftColor
            },
            // Transform
            transform: computedStyle.transform !== 'none' ? {
                transform: computedStyle.transform,
                transformOrigin: computedStyle.transformOrigin
            } : null
        };

        result.elements.push(elementInfo);

        // Extract text node metrics
        if (el.childNodes.length > 0) {
            el.childNodes.forEach((child, childIndex) => {
                if (child.nodeType === Node.TEXT_NODE && child.textContent.trim()) {
                    const range = document.createRange();
                    range.selectNodeContents(child);
                    const rects = range.getClientRects();
                    
                    const textMetric = {
                        parentIndex: index,
                        childIndex: childIndex,
                        text: child.textContent,
                        rects: Array.from(rects).map(r => ({
                            x: r.x,
                            y: r.y,
                            width: r.width,
                            height: r.height
                        }))
                    };
                    
                    // Try to get individual glyph positions using canvas
                    try {
                        const canvas = document.createElement('canvas');
                        const ctx = canvas.getContext('2d');
                        ctx.font = `${computedStyle.fontStyle} ${computedStyle.fontWeight} ${computedStyle.fontSize} ${computedStyle.fontFamily}`;
                        
                        const text = child.textContent.trim();
                        textMetric.glyphs = [];
                        let currentX = 0;
                        
                        for (let i = 0; i < text.length; i++) {
                            const char = text[i];
                            const metrics = ctx.measureText(char);
                            const fullMetrics = ctx.measureText(text.substring(0, i + 1));
                            
                            textMetric.glyphs.push({
                                char: char,
                                charCode: char.charCodeAt(0),
                                x: currentX,
                                width: metrics.width,
                                advanceWidth: metrics.width
                            });
                            
                            currentX += metrics.width;
                        }
                        
                        textMetric.totalWidth = ctx.measureText(text).width;
                    } catch (e) {
                        textMetric.glyphError = e.message;
                    }
                    
                    result.textMetrics.push(textMetric);
                }
            });
        }
    });

    // Get loaded fonts
    if (document.fonts) {
        document.fonts.forEach(font => {
            result.fonts.push({
                family: font.family,
                style: font.style,
                weight: font.weight,
                status: font.status
            });
        });
    }

    // Get document info
    result.document = {
        title: document.title,
        characterSet: document.characterSet,
        contentType: document.contentType,
        documentMode: document.documentMode,
        compatMode: document.compatMode
    };

    // Output as JSON
    return JSON.stringify(result, null, 2);
})();
