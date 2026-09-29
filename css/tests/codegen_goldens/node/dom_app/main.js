'use strict';
const { renderUi } = require('./styles');

const renderUiValue = renderUi();
console.log(`renderUi: ${renderUiValue.raw.len} properties`);
