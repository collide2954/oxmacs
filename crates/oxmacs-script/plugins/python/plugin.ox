(plugin python
  :version "0.1.0"
  :opt-in true
  :summary "Python with highlighting, indentation and the basedpyright, pyright or pylsp language server"
  :modes [{title: "Python" extensions: ["py" "pyi" "pyw"] comment: "#"
           roots: ["pyproject.toml" "setup.py" "setup.cfg" "requirements.txt"]}])
