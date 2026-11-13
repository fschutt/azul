'use strict';
const { styleBtn } = require('./styles');

const styleBtnValue = styleBtn();
console.log(`styleBtn: ${styleBtnValue.raw.len} properties`);
