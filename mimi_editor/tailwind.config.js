/** @type {import('tailwindcss').Config} */
module.exports = {
	important: true,
	content: [
		'./extensions/MimiIncubator/includes/**/*.php',
		'./extensions/MimiIncubator/resources/**/*.js'
	],
	corePlugins: { preflight: false }
};
