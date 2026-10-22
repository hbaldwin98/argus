//! The grammars Argus knows, by file extension or name: each an npm package's
//! prebuilt WebAssembly parser and highlight queries, pinned to a version and
//! to the SHA-256 of every file, so what installs is what was checked here.
//! Several languages' queries are only their additions to another's
//! (TypeScript to JavaScript's, C++ to C's), and list both.
//!
//! The same list the page pins (`ts:code/grammar/known.ts`). `cargo test -p
//! argus-desktop-core -- --ignored the_pins_are_the_files_npm_serves`
//! downloads each again, checks its hash, and that it loads and its queries
//! compile.

/// One file of a package, with the hash and size it was checked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    /// `name@version`, scoped names included.
    pub package: &'static str,
    /// The path inside the package.
    pub file: &'static str,
    /// Lower-case hex.
    pub sha256: &'static str,
    pub size: u64,
}

impl Pinned {
    /// jsDelivr serves a package's files, by exact version, at this address.
    pub fn url(&self) -> String {
        format!("https://cdn.jsdelivr.net/npm/{}/{}", self.package, self.file)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Known {
    pub name: &'static str,
    pub label: &'static str,
    /// Lower case, without the dot.
    pub exts: &'static [&'static str],
    /// Whole file names, for files with no extension of their own.
    pub files: &'static [&'static str],
    /// The npm package, as `name@version`.
    pub package: &'static str,
    pub wasm: Pinned,
    pub queries: &'static [Pinned],
}

/// The grammar Argus knows for `path`, by its name or its extension: one pass
/// in list order, the first entry that matches either. The whole name is
/// compared as written; only the extension is lower-cased, and a leading dot
/// is no extension (`.bashrc` has none).
pub fn known_for(path: &str) -> Option<&'static Known> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = match name.rfind('.') {
        Some(dot) if dot > 0 => name[dot + 1..].to_lowercase(),
        _ => String::new(),
    };
    KNOWN.iter().find(|k| k.files.contains(&name) || (!ext.is_empty() && k.exts.contains(&ext.as_str())))
}

pub const KNOWN: &[Known] = &[
    Known {
        name: "rust",
        label: "Rust",
        exts: &["rs"],
        files: &[],
        package: "tree-sitter-rust@0.24.0",
        wasm: Pinned { package: "tree-sitter-rust@0.24.0", file: "tree-sitter-rust.wasm", sha256: "f65f354215611fd94ad34134b3427eb3d58cbb745df7b6509ba722184db73d57", size: 1102547 },
        queries: &[
            Pinned { package: "tree-sitter-rust@0.24.0", file: "queries/highlights.scm", sha256: "0f0343107f14a7690157f51090a979eb8f8bfe4eada7c61763ddb4c54b1311d1", size: 3527 },
        ],
    },
    Known {
        name: "python",
        label: "Python",
        exts: &["py", "pyi"],
        files: &[],
        package: "tree-sitter-python@0.25.0",
        wasm: Pinned { package: "tree-sitter-python@0.25.0", file: "tree-sitter-python.wasm", sha256: "16108b50df4ee9a30168794252ab55e7c93bfc5765d7fa0aa3e335752c515f47", size: 457883 },
        queries: &[
            Pinned { package: "tree-sitter-python@0.25.0", file: "queries/highlights.scm", sha256: "a6708f209381618e2b398972c8f1ccd892f0c064eab35a2a3f911c3e22e79a7e", size: 1957 },
        ],
    },
    Known {
        name: "go",
        label: "Go",
        exts: &["go"],
        files: &[],
        package: "tree-sitter-go@0.25.0",
        wasm: Pinned { package: "tree-sitter-go@0.25.0", file: "tree-sitter-go.wasm", sha256: "9504573f352b20be7f2f1911754d710622aedc15afff16d5ed8fb5645681aee7", size: 217182 },
        queries: &[
            Pinned { package: "tree-sitter-go@0.25.0", file: "queries/highlights.scm", sha256: "81182c986547eba7fa6316e82dfd621fb13b8fc89efac85432aee51a48ed0896", size: 1421 },
        ],
    },
    Known {
        name: "javascript",
        label: "JavaScript",
        exts: &["js", "mjs", "cjs", "jsx"],
        files: &[],
        package: "tree-sitter-javascript@0.25.0",
        wasm: Pinned { package: "tree-sitter-javascript@0.25.0", file: "tree-sitter-javascript.wasm", sha256: "5fb488d0cabb4775a594bab85682de5ad6ce83c0d6ac997a9f82dd084d571240", size: 411770 },
        queries: &[
            Pinned { package: "tree-sitter-javascript@0.25.0", file: "queries/highlights.scm", sha256: "d3630ae6dc9b2b27b230b5f8bb92b05cd491fb12bff353dae62a0a6d780461ee", size: 2739 },
            Pinned { package: "tree-sitter-javascript@0.25.0", file: "queries/highlights-jsx.scm", sha256: "0cad1712acaf40804bc5dc7c8476858752c1f549cdf7d8c63c747317be0f8fa6", size: 437 },
        ],
    },
    Known {
        name: "typescript",
        label: "TypeScript",
        exts: &["ts", "mts", "cts"],
        files: &[],
        package: "tree-sitter-typescript@0.23.2",
        wasm: Pinned { package: "tree-sitter-typescript@0.23.2", file: "tree-sitter-typescript.wasm", sha256: "778025db5a8be0e70f8ccc3671e486dfeddd048c25d9e8a70c26de2e1bf6f97d", size: 1413849 },
        queries: &[
            Pinned { package: "tree-sitter-javascript@0.25.0", file: "queries/highlights.scm", sha256: "d3630ae6dc9b2b27b230b5f8bb92b05cd491fb12bff353dae62a0a6d780461ee", size: 2739 },
            Pinned { package: "tree-sitter-typescript@0.23.2", file: "queries/highlights.scm", sha256: "e0c35adb819127bfd4f853fac5419e7d8ba44760246201d04a4a5ce0228a10c5", size: 515 },
        ],
    },
    Known {
        name: "tsx",
        label: "TSX",
        exts: &["tsx"],
        files: &[],
        package: "tree-sitter-typescript@0.23.2",
        wasm: Pinned { package: "tree-sitter-typescript@0.23.2", file: "tree-sitter-tsx.wasm", sha256: "79e5da75ea62855a0cd67177685f0164eac87d5f630b3cbe1e0a099751ad30f8", size: 1445638 },
        queries: &[
            Pinned { package: "tree-sitter-javascript@0.25.0", file: "queries/highlights.scm", sha256: "d3630ae6dc9b2b27b230b5f8bb92b05cd491fb12bff353dae62a0a6d780461ee", size: 2739 },
            Pinned { package: "tree-sitter-javascript@0.25.0", file: "queries/highlights-jsx.scm", sha256: "0cad1712acaf40804bc5dc7c8476858752c1f549cdf7d8c63c747317be0f8fa6", size: 437 },
            Pinned { package: "tree-sitter-typescript@0.23.2", file: "queries/highlights.scm", sha256: "e0c35adb819127bfd4f853fac5419e7d8ba44760246201d04a4a5ce0228a10c5", size: 515 },
        ],
    },
    Known {
        name: "c",
        label: "C",
        exts: &["c", "h"],
        files: &[],
        package: "tree-sitter-c@0.24.1",
        wasm: Pinned { package: "tree-sitter-c@0.24.1", file: "tree-sitter-c.wasm", sha256: "c852c2a85ebf2beb636aa3b0ef7f7e70458684d74f6741b20dcb296885bed9f9", size: 625918 },
        queries: &[
            Pinned { package: "tree-sitter-c@0.24.1", file: "queries/highlights.scm", sha256: "3378d854dda695b2b282b9468247524ed4271ef74af691033d2e36883379409c", size: 1432 },
        ],
    },
    Known {
        name: "cpp",
        label: "C++",
        exts: &["cc", "cpp", "cxx", "hh", "hpp", "hxx"],
        files: &[],
        package: "tree-sitter-cpp@0.23.4",
        wasm: Pinned { package: "tree-sitter-cpp@0.23.4", file: "tree-sitter-cpp.wasm", sha256: "174eb0deb75b2ec7881bcacda9f995648d8e683956e5c2267e69ab6dc503fcbf", size: 3434931 },
        queries: &[
            Pinned { package: "tree-sitter-c@0.24.1", file: "queries/highlights.scm", sha256: "3378d854dda695b2b282b9468247524ed4271ef74af691033d2e36883379409c", size: 1432 },
            Pinned { package: "tree-sitter-cpp@0.23.4", file: "queries/highlights.scm", sha256: "52136576a9a9dacd9e95a8de0f351689bf46140738572ab4e9f24c9278e6b458", size: 939 },
        ],
    },
    Known {
        name: "c_sharp",
        label: "C#",
        exts: &["cs"],
        files: &[],
        package: "tree-sitter-c-sharp@0.23.5",
        wasm: Pinned { package: "tree-sitter-c-sharp@0.23.5", file: "tree-sitter-c_sharp.wasm", sha256: "6f69e1cae44e1c32c1eccc170dc5a9778fb94ff716f71113fe1f8c4299aa2f40", size: 5350581 },
        queries: &[
            Pinned { package: "tree-sitter-c-sharp@0.23.5", file: "queries/highlights.scm", sha256: "ab8a9930aeeee70fa2dbfde82e4763170b7e826bc642338ad0683772c20c060f", size: 2783 },
        ],
    },
    Known {
        name: "java",
        label: "Java",
        exts: &["java"],
        files: &[],
        package: "tree-sitter-java@0.23.5",
        wasm: Pinned { package: "tree-sitter-java@0.23.5", file: "tree-sitter-java.wasm", sha256: "4fdeac4ca6ca089f06c6f7e562abcac1733cd465728cc7031ebb73c2019122c4", size: 414641 },
        queries: &[
            Pinned { package: "tree-sitter-java@0.23.5", file: "queries/highlights.scm", sha256: "576c0df8df0b116cd642140ddc508c01f9d3283582afd8581c1f35caf4d71386", size: 2064 },
        ],
    },
    Known {
        name: "json",
        label: "JSON",
        exts: &["json"],
        files: &[],
        package: "tree-sitter-json@0.24.8",
        wasm: Pinned { package: "tree-sitter-json@0.24.8", file: "tree-sitter-json.wasm", sha256: "d2119fb98d5912719b13f9458574f8608d2d29dfbe45f6be1f860ea1fe2a2405", size: 5596 },
        queries: &[
            Pinned { package: "tree-sitter-json@0.24.8", file: "queries/highlights.scm", sha256: "0511524465b56aed122580792254e68b6abbbfde7119f1d02b135acbe278233f", size: 172 },
        ],
    },
    Known {
        name: "css",
        label: "CSS",
        exts: &["css"],
        files: &[],
        package: "tree-sitter-css@0.25.0",
        wasm: Pinned { package: "tree-sitter-css@0.25.0", file: "tree-sitter-css.wasm", sha256: "8a23977fe271357cce6f254ef88c9bebf3854602d8046605aef6a45c02135c59", size: 128668 },
        queries: &[
            Pinned { package: "tree-sitter-css@0.25.0", file: "queries/highlights.scm", sha256: "23f7948a3817a0d06d7120158c3bee4ec5b58daa77e524985a4738b364db17b2", size: 1177 },
        ],
    },
    Known {
        name: "html",
        label: "HTML",
        exts: &["html", "htm"],
        files: &[],
        package: "tree-sitter-html@0.23.2",
        wasm: Pinned { package: "tree-sitter-html@0.23.2", file: "tree-sitter-html.wasm", sha256: "c48fcd82c7ea8bf943180088ba7f28c48b2bb5287874179168bf9d31e394cf85", size: 18385 },
        queries: &[
            Pinned { package: "tree-sitter-html@0.23.2", file: "queries/highlights.scm", sha256: "1ebb3811a8cdc054385b847a3aac6fbf7079faefd5b3dfab5bbad256cd5afdcf", size: 197 },
        ],
    },
    Known {
        name: "bash",
        label: "Bash",
        exts: &["sh", "bash", "zsh"],
        files: &[".bashrc", ".bash_profile", ".zshrc", ".profile"],
        package: "tree-sitter-bash@0.25.1",
        wasm: Pinned { package: "tree-sitter-bash@0.25.1", file: "tree-sitter-bash.wasm", sha256: "8292919c88a0f7d3fb31d0cd0253ca5a9531bc1ede82b0537f2c63dd8abe6a7a", size: 1358224 },
        queries: &[
            Pinned { package: "tree-sitter-bash@0.25.1", file: "queries/highlights.scm", sha256: "b74220d954f485b7626d2b2b61f37b522e12eb1830803e388e57dd797dc99f11", size: 577 },
        ],
    },
    Known {
        name: "ruby",
        label: "Ruby",
        exts: &["rb", "gemspec", "rake"],
        files: &["Gemfile", "Rakefile"],
        package: "tree-sitter-ruby@0.23.1",
        wasm: Pinned { package: "tree-sitter-ruby@0.23.1", file: "tree-sitter-ruby.wasm", sha256: "09a96427d7c72f0613ed470cd9812223fc4a91d6a9c025c0235cc6bd59ff96f4", size: 2106352 },
        queries: &[
            Pinned { package: "tree-sitter-ruby@0.23.1", file: "queries/highlights.scm", sha256: "0858de9feece6dcd3408a541f995a34918d587ac2c552096026b0e47e2b332e6", size: 2377 },
        ],
    },
    Known {
        name: "scala",
        label: "Scala",
        exts: &["scala", "sc"],
        files: &[],
        package: "tree-sitter-scala@0.24.0",
        wasm: Pinned { package: "tree-sitter-scala@0.24.0", file: "tree-sitter-scala.wasm", sha256: "b7ec2bb29c19827abcefd18ed5cb5a43596009f96a5d53c5b9d1f9676d7521c3", size: 3786700 },
        queries: &[
            Pinned { package: "tree-sitter-scala@0.24.0", file: "queries/highlights.scm", sha256: "8b3dc3bd20e05112854d3f4bd9a564fe53b762db89db9760d83d4f8e3828d0ab", size: 4345 },
        ],
    },
    Known {
        name: "haskell",
        label: "Haskell",
        exts: &["hs"],
        files: &[],
        package: "tree-sitter-haskell@0.23.1",
        wasm: Pinned { package: "tree-sitter-haskell@0.23.1", file: "tree-sitter-haskell.wasm", sha256: "37a6b07b1a838d02ffb4f4c2a06863637a8efe48432d60a275f50f1d08f1092c", size: 3805902 },
        queries: &[
            Pinned { package: "tree-sitter-haskell@0.23.1", file: "queries/highlights.scm", sha256: "f8232b2c9b79ca3038c08c0b13602776019039d8e7668c60c18dcbc373c32e16", size: 8294 },
        ],
    },
    Known {
        name: "julia",
        label: "Julia",
        exts: &["jl"],
        files: &[],
        package: "tree-sitter-julia@0.23.1",
        wasm: Pinned { package: "tree-sitter-julia@0.23.1", file: "tree-sitter-julia.wasm", sha256: "e0f52c36eadf0299e46fccd6715c760d35eaa3f09721bec38633da551ac9e781", size: 6223352 },
        queries: &[
            Pinned { package: "tree-sitter-julia@0.23.1", file: "queries/highlights.scm", sha256: "6963a8450d144a18761a7b4e68787af497d06e51e92c31c15893cc583a19d0a4", size: 5865 },
        ],
    },
    Known {
        name: "toml",
        label: "TOML",
        exts: &["toml"],
        files: &["Cargo.lock"],
        package: "@tree-sitter-grammars/tree-sitter-toml@0.7.0",
        wasm: Pinned { package: "@tree-sitter-grammars/tree-sitter-toml@0.7.0", file: "tree-sitter-toml.wasm", sha256: "1ac6a83826c35a68857f8325e00c78f6bcbef4eb6db3931a6cf3041f76e5e09f", size: 24040 },
        queries: &[
            Pinned { package: "@tree-sitter-grammars/tree-sitter-toml@0.7.0", file: "queries/highlights.scm", sha256: "2fb5c61d33a70389312254c9c392e2c5dd6313d958d6e5f8f74cadd5e0811707", size: 501 },
        ],
    },
    Known {
        name: "zig",
        label: "Zig",
        exts: &["zig"],
        files: &[],
        package: "@tree-sitter-grammars/tree-sitter-zig@1.1.2",
        wasm: Pinned { package: "@tree-sitter-grammars/tree-sitter-zig@1.1.2", file: "tree-sitter-zig.wasm", sha256: "54b3b83dd9c62da5815f06132bc3fc914d9dcc780370b32416446a0b7969e8c6", size: 691726 },
        queries: &[
            Pinned { package: "@tree-sitter-grammars/tree-sitter-zig@1.1.2", file: "queries/highlights.scm", sha256: "27a7ffc77086ec6394c04e59b42f0da4c9d3c294a0225bc155c80305ee54ca86", size: 3345 },
        ],
    },
    Known {
        name: "lua",
        label: "Lua",
        exts: &["lua"],
        files: &[],
        package: "@tree-sitter-grammars/tree-sitter-lua@0.4.1",
        wasm: Pinned { package: "@tree-sitter-grammars/tree-sitter-lua@0.4.1", file: "tree-sitter-lua.wasm", sha256: "6d95607fc7d78964cfdf065ccb1ba76be5ed217c5ec0d0a3cace13c59fa1ae43", size: 49488 },
        queries: &[
            Pinned { package: "@tree-sitter-grammars/tree-sitter-lua@0.4.1", file: "queries/highlights.scm", sha256: "0ddb74b9e476d0b183a39a275dc9a6a370a69fb94bc9f7e6fac4c2cbe64f7b75", size: 2893 },
        ],
    },
    Known {
        name: "elixir",
        label: "Elixir",
        exts: &["ex", "exs"],
        files: &[],
        package: "tree-sitter-elixir@0.3.5",
        wasm: Pinned { package: "tree-sitter-elixir@0.3.5", file: "tree-sitter-elixir.wasm", sha256: "ed99093c548c12d43f7e337fd3440e9e2daa2ec671a5e29aadb6c6dcb2232a62", size: 1412267 },
        queries: &[
            Pinned { package: "tree-sitter-elixir@0.3.5", file: "queries/highlights.scm", sha256: "5ee9952a99ce2f0c3dcdf4478daf38f259dd8df4e7f9a892e9f1c03ccdf61ea6", size: 4230 },
        ],
    },
    Known {
        name: "php",
        label: "PHP",
        exts: &["php"],
        files: &[],
        package: "tree-sitter-php@0.24.2",
        wasm: Pinned { package: "tree-sitter-php@0.24.2", file: "tree-sitter-php.wasm", sha256: "d4df6a6ff08c87c3ec4f9cbb785fe09998a0cb570e03f57d7b19b3acfb146aa7", size: 1058041 },
        queries: &[
            Pinned { package: "tree-sitter-php@0.24.2", file: "queries/highlights.scm", sha256: "a2a7367659cff3b4be09961c8117d69a9b8703bfc266f8092e089b1760f197e9", size: 3207 },
        ],
    },
];

#[cfg(test)]
mod tests;
