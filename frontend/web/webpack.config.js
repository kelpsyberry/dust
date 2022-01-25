const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const { CleanWebpackPlugin } = require("clean-webpack-plugin");
const { resolve, join } = require("path");
const MiniCssExtractPlugin = require("mini-css-extract-plugin");
const CopyPlugin = require("copy-webpack-plugin");

const fontawesomePath = require.resolve("@fortawesome/fontawesome-free");

const src = resolve(__dirname, "src");
const emuSrc = resolve(src, "emu");
const dist = resolve(__dirname, "dist");

const mode = "development";
const sourceMap = mode === "development";
const optimize = mode === "production";

const plugins = [
    new WasmPackPlugin({
        crateDirectory: resolve(__dirname, "crate"),
        watchDirectories: [resolve(__dirname, "../../core")],
        outDir: resolve(__dirname, "pkg"),
        forceMode: "production",
        pluginLogLevel: "warn",
    }),
    new CleanWebpackPlugin(),
    new MiniCssExtractPlugin(),
    new CopyPlugin({
        patterns: [
            resolve(src, "index.html"),
            resolve(src, "resources"),
            { from: resolve(__dirname, "../../game_db.json"), to: "resources/game_db.json" },
            { from: join(fontawesomePath, "../../css"), to: "fontawesome/css" },
            {
                from: join(fontawesomePath, "../../webfonts"),
                to: "fontawesome/webfonts",
            },
        ],
    }),
];

if (optimize) {
    plugins.push(
        new (require("optimize-css-assets-webpack-plugin"))({
            cssProcessorPluginOptions: {
                preset: ["default", { discardComments: true }],
            },
        })
    );
}

function pluginsForDir(dir) {
    if (optimize) {
        return plugins;
    }
    return plugins.concat(
        new (require("fork-ts-checker-webpack-plugin"))({
            typescript: {
                configFile: join(dir, "tsconfig.json"),
            },
        })
    );
}

const baseConfig = {
    context: resolve(__dirname),
    devtool: sourceMap ? "source-map" : undefined,
    plugins,
    module: {
        rules: [
            {
                test: /\.less$/i,
                use: [
                    MiniCssExtractPlugin.loader,
                    {
                        loader: "css-loader",
                        options: {
                            sourceMap,
                        },
                    },
                    {
                        loader: "less-loader",
                        options: {
                            sourceMap,
                        },
                    },
                ],
            },
            {
                test: /\.css$/i,
                use: [
                    MiniCssExtractPlugin.loader,
                    {
                        loader: "css-loader",
                        options: {
                            sourceMap,
                        },
                    },
                ],
            },
            {
                test: /\.(eot|svg|ttf|woff|woff2|png|map)$/i,
                type: "asset/resource",
                generator: {
                    filename: "[name].[ext]",
                },
            },
            {
                test: /\.tsx?$/i,
                use: {
                    loader: "ts-loader",
                    options: {
                        transpileOnly: !optimize,
                        configFile: "tsconfig.json",
                        compilerOptions: {
                            sourceMap,
                        },
                    },
                },
            },
        ],
    },
    resolve: {
        extensions: [".ts", ".tsx", ".js", ".json"],
    },
    output: {
        filename: "[name].bundle.js",
        path: dist,
    },
    experiments: {
        syncWebAssembly: true,
    },
    optimization: optimize
        ? {
              minimize: true,
              minimizer: [
                  new (require("css-minimizer-webpack-plugin"))(),
                  "...",
              ],
          }
        : {},
    watchOptions: {
        ignored: ["**/node_modules", "dist"],
    },
    mode,
};

module.exports = [
    Object.assign(
        {
            plugins: pluginsForDir(resolve(src, "ui")),
            entry: {
                ui: [
                    resolve(src, "styles/main.less"),
                    resolve(src, "ui/ui.ts"),
                ],
            },
            devServer: {
                static: [dist],
                compress: true,
                host: "0.0.0.0",
                port: 2626,
                hot: false,
                liveReload: false,
                webSocketServer: false,
            },
        },
        baseConfig
    ),
    Object.assign(
        {
            plugins: pluginsForDir(resolve(src, "emu")),
            entry: {
                emu: resolve(src, "emu/emu.ts"),
            },
            target: "webworker",
        },
        baseConfig
    ),
];
