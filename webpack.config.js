const path = require("path");

module.exports = {
  entry: "./static/rustw.ts",
  output: {
    filename: "rustw.out.js",
    libraryTarget: "var",
    library: "Rustw",
    path: path.resolve(__dirname, "static"),
  },
  mode: "development",
  resolve: {
    extensions: [".js", ".ts", ".tsx"]
  },
  module: {
    rules: [
    {
      test: /\.js$/,
      exclude: /node_modules/,
      loader: "babel-loader"
    },
    {
      test: /\.tsx?$/,
      exclude: /node_modules/,
      loader: "ts-loader"
    }]
  },
  devtool: "source-map",
  devServer: {
    publicPath: "/static/",
    port: 9000,
    watchContentBase: true,
    historyApiFallback: {
      disableDotRule: true
    },
    proxy: [
      {
        context: [
          "/build",
          "/config",
          "/find",
          "/search",
          "/src",
          "/status",
          "/symbol_roots",
          "/tree"
        ],
        target: "http://localhost:8080"
      },
    ]
  },
}
