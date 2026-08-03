const path = require("path");
const { getDefaultConfig } = require("expo/metro-config");

/** @type {import('expo/metro-config').MetroConfig} */
const config = getDefaultConfig(__dirname);

// Watch shared light-client / brand sources outside the Expo app root.
config.watchFolders = [path.resolve(__dirname, "..")];

module.exports = config;
