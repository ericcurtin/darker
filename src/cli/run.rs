//! `darker run` command implementation

use crate::runtime::container::Container;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;

/// Arguments for the `run` command
#[derive(Args)]
pub struct RunArgs {
    /// Image to run
    pub image: String,

    /// Command to run in the container
    pub command: Vec<String>,

    /// Container name
    #[arg(long)]
    pub name: Option<String>,

    /// Run container in detached mode
    #[arg(short, long)]
    pub detach: bool,

    /// Automatically remove container when it exits
    #[arg(long)]
    pub rm: bool,

    /// Set environment variables
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Bind mount a volume
    #[arg(short, long)]
    pub volume: Vec<String>,

    /// Working directory inside the container
    #[arg(short, long)]
    pub workdir: Option<String>,

    /// Username or UID
    #[arg(short, long)]
    pub user: Option<String>,

    /// Keep STDIN open even if not attached
    #[arg(short, long)]
    pub interactive: bool,

    /// Allocate a pseudo-TTY
    #[arg(short, long)]
    pub tty: bool,

    /// Override the default entrypoint
    #[arg(long)]
    pub entrypoint: Option<String>,

    /// Add a custom host-to-IP mapping
    #[arg(long)]
    pub add_host: Vec<String>,

    /// Container hostname
    #[arg(long)]
    pub hostname: Option<String>,

    /// Publish container's port(s) to the host (no-op for host networking)
    #[arg(short, long)]
    pub publish: Vec<String>,

    /// Run container in read-only mode
    #[arg(long)]
    pub read_only: bool,
}

/// Execute the `run` command
pub async fn execute(args: RunArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    paths.ensure_directories()?;

    // Check if image exists locally, if not try to pull it
    let image_store = crate::storage::images::ImageStore::new(&paths)?;

    // Special handling for scratch image - it's a virtual empty image
    let is_scratch = args.image == "scratch";

    let image_id = if is_scratch {
        // scratch is a virtual empty image with no layers
        "scratch".to_string()
    } else {
        let image_ref = crate::image::oci::ImageReference::parse(&args.image)?;
        match image_store.find_image(&image_ref) {
            Some(id) => id,
            None => {
                eprintln!("Unable to find image '{}' locally", args.image);
                eprintln!("Pulling from registry...");

                // Pull the image
                let registry = crate::image::registry::RegistryClient::new()?;
                let pulled_id = registry.pull(&image_ref, &paths).await?;
                eprintln!("Successfully pulled {}", args.image);
                pulled_id
            }
        }
    };

    // Create container
    let container_store = crate::storage::containers::ContainerStore::new(&paths)?;
    let container_name = args.name.unwrap_or_else(|| generate_container_name());

    if container_store.exists(&container_name) {
        return Err(DarkerError::ContainerExists(container_name).into());
    }

    let container_id = uuid::Uuid::new_v4().to_string();
    let short_id = &container_id[..12];

    // Load image config (scratch has no config, use defaults)
    let image_config = if is_scratch {
        crate::storage::images::ImageConfig::default()
    } else {
        image_store.load_config(&image_id)?
    };

    // Determine command to run
    let cmd = if !args.command.is_empty() {
        args.command.clone()
    } else if let Some(ref entrypoint) = args.entrypoint {
        vec![entrypoint.clone()]
    } else {
        image_config.cmd().unwrap_or_default()
    };

    // Determine entrypoint
    let entrypoint = args
        .entrypoint
        .clone()
        .or_else(|| image_config.entrypoint().map(|e| e.join(" ")));

    // Determine working directory
    let workdir = args
        .workdir
        .clone()
        .or_else(|| image_config.working_dir().map(String::from))
        .unwrap_or_else(|| "/".to_string());

    // Merge environment variables
    let mut env: Vec<String> = image_config.env().unwrap_or_default();
    env.extend(args.env.clone());

    // Create container config
    let config = crate::storage::containers::ContainerConfig {
        id: container_id.clone(),
        name: container_name.clone(),
        image: args.image.clone(),
        image_id: image_id.clone(),
        command: cmd.clone(),
        entrypoint,
        env,
        working_dir: workdir,
        volumes: args.volume.clone(),
        user: args.user.clone(),
        hostname: args.hostname.clone().unwrap_or_else(|| short_id.to_string()),
        tty: args.tty,
        stdin_open: args.interactive,
        read_only: args.read_only,
        auto_remove: args.rm,
        created: chrono::Utc::now(),
    };

    container_store.create(&config)?;

    // Set up rootfs
    let rootfs = crate::filesystem::rootfs::RootFs::new(&paths, &container_id)?;
    rootfs.setup(&image_id, &args.volume)?;

    // Create and start container
    let mut container = Container::new(config, &paths)?;

    if args.detach {
        container.start_detached().await?;
        println!("{}", container_id);
    } else {
        let exit_code = container.run(args.tty, args.interactive).await?;

        if args.rm {
            container_store.remove(&container_id)?;
            rootfs.cleanup()?;
        }

        std::process::exit(exit_code);
    }

    Ok(())
}

/// Generate a random container name
fn generate_container_name() -> String {
    use rand::seq::SliceRandom;

    let adjectives = [
        "admiring",
        "agitated",
        "amazing",
        "angry",
        "awesome",
        "beautiful",
        "bold",
        "boring",
        "brave",
        "busy",
        "charming",
        "clever",
        "cool",
        "cranky",
        "crazy",
        "dazzling",
        "determined",
        "eager",
        "elastic",
        "elegant",
        "epic",
        "exciting",
        "fervent",
        "festive",
        "flamboyant",
        "focused",
        "friendly",
        "funny",
        "gallant",
        "gifted",
        "goofy",
        "gracious",
        "happy",
        "hardcore",
        "hopeful",
        "hungry",
        "inspiring",
        "intelligent",
        "interesting",
        "jolly",
        "jovial",
        "keen",
        "kind",
        "laughing",
        "loving",
        "lucid",
        "magical",
        "modest",
        "musing",
        "mystifying",
        "naughty",
        "nervous",
        "nice",
        "nifty",
        "nostalgic",
        "objective",
        "optimistic",
        "peaceful",
        "pedantic",
        "pensive",
        "practical",
        "priceless",
        "quirky",
        "quizzical",
        "recursing",
        "relaxed",
        "reverent",
        "romantic",
        "sad",
        "serene",
        "sharp",
        "silly",
        "sleepy",
        "stoic",
        "strange",
        "stupefied",
        "suspicious",
        "sweet",
        "tender",
        "thirsty",
        "trusting",
        "upbeat",
        "vibrant",
        "vigilant",
        "vigorous",
        "wizardly",
        "wonderful",
        "xenodochial",
        "youthful",
        "zealous",
        "zen",
    ];

    let names = [
        "albattani",
        "allen",
        "austin",
        "babbage",
        "bell",
        "benz",
        "bohr",
        "booth",
        "borg",
        "bouman",
        "brown",
        "buck",
        "burnell",
        "cannon",
        "carson",
        "cerf",
        "chandrasekhar",
        "chatelet",
        "clarke",
        "colden",
        "cori",
        "cray",
        "curie",
        "darwin",
        "davinci",
        "diffie",
        "dijkstra",
        "driscoll",
        "dubinsky",
        "easley",
        "edison",
        "einstein",
        "elgamal",
        "elion",
        "engelbart",
        "euler",
        "faraday",
        "feistel",
        "fermat",
        "fermi",
        "feynman",
        "franklin",
        "gagarin",
        "galileo",
        "galois",
        "gates",
        "gauss",
        "germain",
        "goldberg",
        "goldstine",
        "goldwasser",
        "golick",
        "goodall",
        "gould",
        "greider",
        "grothendieck",
        "haibt",
        "hamilton",
        "haslett",
        "hawking",
        "heisenberg",
        "hellman",
        "hermann",
        "heyrovsky",
        "hodgkin",
        "hofstadter",
        "hoover",
        "hopper",
        "hugle",
        "hypatia",
        "jackson",
        "jang",
        "jennings",
        "jepsen",
        "jobs",
        "johnson",
        "joliot",
        "jones",
        "kalam",
        "kapitsa",
        "kare",
        "keldysh",
        "keller",
        "kepler",
        "khorana",
        "kilby",
        "kirch",
        "knuth",
        "kowalevski",
        "lamarr",
        "lamport",
        "leakey",
        "leavitt",
        "lehmann",
        "lewin",
        "liskov",
        "lovelace",
        "lumiere",
        "mahavira",
        "margulis",
        "matsumoto",
        "maxwell",
        "mayer",
        "mccarthy",
        "mcclintock",
        "mclaren",
        "mclean",
        "mcnulty",
        "meitner",
        "mendel",
        "mendeleev",
        "merkle",
        "mestorf",
        "mirzakhani",
        "montalcini",
        "moore",
        "morse",
        "murdock",
        "napier",
        "nash",
        "neumann",
        "newton",
        "nightingale",
        "nobel",
        "noether",
        "northcutt",
        "noyce",
        "panini",
        "pare",
        "pascal",
        "pasteur",
        "payne",
        "perlman",
        "pike",
        "poincare",
        "poitras",
        "proskuriakova",
        "ptolemy",
        "raman",
        "ramanujan",
        "rhodes",
        "ride",
        "ritchie",
        "robinson",
        "roentgen",
        "rosalind",
        "rubin",
        "saha",
        "sammet",
        "sanderson",
        "satoshi",
        "shamir",
        "shannon",
        "shaw",
        "shirley",
        "shockley",
        "shtern",
        "sinoussi",
        "snyder",
        "solomon",
        "spence",
        "stallman",
        "stonebraker",
        "sutherland",
        "swanson",
        "swartz",
        "swirles",
        "taussig",
        "tesla",
        "tharp",
        "thompson",
        "torvalds",
        "tu",
        "turing",
        "varahamihira",
        "vaughan",
        "villani",
        "visvesvaraya",
        "volhard",
        "wescoff",
        "wilbur",
        "wiles",
        "williams",
        "williamson",
        "wilson",
        "wing",
        "wozniak",
        "wright",
        "wu",
        "yalow",
        "yonath",
        "zhukovsky",
    ];

    let mut rng = rand::thread_rng();
    let adj = adjectives.choose(&mut rng).unwrap_or(&"happy");
    let name = names.choose(&mut rng).unwrap_or(&"darwin");

    format!("{}_{}", adj, name)
}
