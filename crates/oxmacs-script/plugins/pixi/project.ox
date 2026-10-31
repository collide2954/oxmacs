(module
  (import (ox string) (ox path) (ox file) (ox process) (ox json) (ox wsl))
  (export manifest-in root-of program variables-of tasks-of environments-of packages-of reason installer))

(def (manifest-in folder)
  "The pixi manifest in folder: its pixi.toml, or a pyproject.toml with a [tool.pixi] table; nil when there is none."
  (let [toml (path/join folder "pixi.toml")
        pyproject (path/join folder "pyproject.toml")]
    (cond
      [(await (file/exists? toml)) toml]
      [(and (await (file/exists? pyproject))
            (string/includes? (await (file/read pyproject)) "[tool.pixi"))
       pyproject]
      [else nil])))

(def (root-of directory)
  "The folder of the pixi project directory lies in, or nil."
  (loop [folder directory]
    (cond
      [(nil? folder) nil]
      [(manifest-in folder) folder]
      [else (let [up (path/parent folder)]
              (if (= up folder) nil (again up)))])))

(def (program root)
  "The pixi to run for the project at root: inside a WSL distribution its own, found there, else the one on PATH or where pixi's installer puts it; nil when there is none."
  (if (wsl/place root)
    "pixi"
    (or (process/which "pixi")
        (let [installed (path/join (path/home) ".pixi" "bin" (if (= (process/os) :windows) "pixi.exe" "pixi"))]
          (when (await (file/exists? installed)) installed)))))

(def (variables-of hook)
  "The variables pixi shell-hook --json gives an environment."
  (get (json/read hook) :environment_variables {}))

(def (tasks-of listing environment)
  "The tasks pixi task list --json gives for environment, as maps of :name, :command and :description."
  (let [found (or (find (fn [entry] (= entry.environment environment)) (json/read listing)) {})]
    (vec (for [feature (get found :features [])
               task (get feature :tasks [])]
           {name: task.name command: (or task.cmd "") description: task.description}))))

(def (environments-of info)
  "The environments pixi info --json describes, as maps of :name, :features, :dependencies and :tasks."
  (vec (map (fn [environment]
              {name: environment.name
               features: (or environment.features [])
               dependencies: (vec (concat (or environment.dependencies []) (or environment.pypi_dependencies [])))
               tasks: (or environment.tasks [])})
            (get (json/read info) :environments_info []))))

(def (packages-of listing)
  "The packages pixi list --json gives, as maps of :name, :version, :kind and :explicit."
  (vec (map (fn [package]
              {name: package.name version: package.version kind: package.kind explicit: (= package.is_explicit true)})
            (json/read listing))))

(def (reason ran)
  "What a failed pixi run said, in a line."
  (let [said (filter (fn [line] (not (string/blank? line)))
                     (string/lines (if (string/blank? ran.stderr) ran.stdout ran.stderr)))]
    (if (empty? said) $"pixi exited with {ran.status}" (string/trim (last said)))))

(def (installer os)
  "The program and arguments that run pixi's official installer on os."
  (if (= os :windows)
    ["powershell" ["-NoProfile" "-ExecutionPolicy" "ByPass" "-c" "irm -useb https://pixi.sh/install.ps1 | iex"]]
    ["sh" ["-c" "curl -fsSL https://pixi.sh/install.sh | sh"]]))

(test "shell-hook's variables become the environment"
  (expect (= (variables-of "{\"environment_variables\": {\"PATH\": \"/envs/default/bin\", \"CONDA_PREFIX\": \"/envs/default\"}}")
             {PATH: "/envs/default/bin" CONDA_PREFIX: "/envs/default"})))

(test "tasks come from every feature of the environment"
  (let [listing "[{\"environment\": \"default\", \"tasks\": [], \"features\": [{\"name\": \"default\", \"tasks\": [{\"name\": \"hello\", \"cmd\": \"echo hi\", \"description\": null}]}, {\"name\": \"test\", \"tasks\": [{\"name\": \"check\", \"cmd\": \"pytest\", \"description\": \"Runs the tests\"}]}]}, {\"environment\": \"cuda\", \"tasks\": [], \"features\": []}]"]
    (expect (= (tasks-of listing "default")
               [{name: "hello" command: "echo hi" description: nil}
                {name: "check" command: "pytest" description: "Runs the tests"}]))
    (expect (= (tasks-of listing "cuda") []))
    (expect (= (tasks-of listing "missing") []))))

(test "environments name their features, dependencies and tasks"
  (expect (= (environments-of "{\"environments_info\": [{\"name\": \"default\", \"features\": [\"default\"], \"dependencies\": [\"zlib\"], \"pypi_dependencies\": [\"rich\"], \"tasks\": [\"hello\"]}]}")
             [{name: "default" features: ["default"] dependencies: ["zlib" "rich"] tasks: ["hello"]}])))

(test "packages say whether the manifest asks for them"
  (expect (= (packages-of "[{\"name\": \"zlib\", \"version\": \"1.3.2\", \"kind\": \"conda\", \"is_explicit\": true}, {\"name\": \"libzlib\", \"version\": \"1.3.2\", \"kind\": \"conda\", \"is_explicit\": false}]")
             [{name: "zlib" version: "1.3.2" kind: "conda" explicit: true}
              {name: "libzlib" version: "1.3.2" kind: "conda" explicit: false}])))

(test "failures say pixi's last word"
  (expect (= (reason {status: 1 stdout: "" stderr: "  × could not solve\n  help: try again\n"}) "help: try again"))
  (expect (= (reason {status: 2 stdout: "" stderr: ""}) "pixi exited with 2")))
