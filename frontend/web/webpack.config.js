const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const { CleanWebpackPlugin } = require("clean-webpack-plugin");
const { resolve } = require("path");
const MiniCssExtractPlugin = require("mini-css-extract-plugin");
const CopyPlugin = require("copy-webpack-plugin");

const fontawesomePath = require.resolve("@fortawesome/fontawesome-free");

const src = resolve(__dirname, "src");
const pkg = resolve(__dirname, "pkg");
const dist = resolve(__dirname, "dist");

const mode = process.env.BUILD_MODE ?? "development";
const sourceMap = mode === "development";
const optimize = mode === "production";

const plugins = [
    new WasmPackPlugin({
        crateDirectory: resolve(__dirname, "crate"),
        watchDirectories: [resolve(__dirname, "../../core")],
        outDir: resolve(__dirname, pkg),
        forceMode: "production",
        pluginLogLevel: "warn",
        extraArgs: "--target web -- . -Zbuild-std=panic_abort,std",
    }),
    new MiniCssExtractPlugin(),
    new CopyPlugin({
        patterns: [
            resolve(src, "index.html"),
            resolve(src, "resources"),
            { from: pkg, to: "pkg" },
            {
                from: resolve(__dirname, "../../game_db.json"),
                to: "resources/game_db.json",
            },
            {
                from: resolve(fontawesomePath, "../../css"),
                to: "fontawesome/css",
            },
            {
                from: resolve(fontawesomePath, "../../webfonts"),
                to: "fontawesome/webfonts",
            },
        ],
    }),
];

function pluginsForDir(dir) {
    if (optimize) {
        return plugins;
    }
    return plugins.concat(
        new (require("fork-ts-checker-webpack-plugin"))({
            typescript: {
                configFile: resolve(dir, "tsconfig.json"),
            },
        })
    );
}

const baseConfig = {
    context: resolve(__dirname),
    devtool: sourceMap ? "source-map" : undefined,
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
            name: "ui",
            plugins: pluginsForDir(resolve(src, "ui")).concat(
                new CleanWebpackPlugin()
            ),
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
                headers: {
                    "Cross-Origin-Opener-Policy": "same-origin",
                    "Cross-Origin-Embedder-Policy": "require-corp"
                },
                https: true,
            },
        },
        baseConfig
    ),
    Object.assign(
        {
            name: "emu",
            plugins: pluginsForDir(resolve(src, "emu")),
            entry: {
                emu: resolve(src, "emu/emu.ts"),
            },
            target: "webworker",
            dependencies: ["ui"],
        },
        baseConfig
    ),
    Object.assign(
        {
            name: "renderer_3d",
            plugins: pluginsForDir(resolve(src, "renderer_3d")),
            entry: {
                renderer_3d: resolve(src, "renderer_3d/renderer_3d.ts"),
            },
            target: "webworker",
            dependencies: ["emu"],
        },
        baseConfig
    ),
];
