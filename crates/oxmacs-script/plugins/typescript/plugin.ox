(plugin typescript
  :version "0.1.0"
  :opt-in true
  :summary "TypeScript and TSX with highlighting, indentation and typescript-language-server"
  :modes [{title: "TypeScript" extensions: ["ts" "mts" "cts"] comment: "//" roots: ["package.json" "tsconfig.json"]}
          {title: "TSX" extensions: ["tsx"] comment: "//" roots: ["package.json" "tsconfig.json"]}])
