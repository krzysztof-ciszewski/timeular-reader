<h1 align="center">Timeular Reader</h1>
<p align="center">Have you bought the expensive <a href="https://timeular.com/tracker">Timeular tracker</a> and don't want to pay on top of that for their propriatery app? This project is for you. With Timeular Reader you can connnect your tracker to your favourite time tracking app.
</p>

## Installation

Download the executable for your platform from [Releases](https://github.com/krzysztof-ciszewski/timeular-reader/releases):

| Platform | Executable |
| --- | --- |
| Windows x64 | `timeular-reader-windows-x64.exe` |
| Linux x64 | `timeular-reader-linux-x64` |
| macOS Intel | `timeular-reader-macos-x64` |
| macOS Apple Silicon | `timeular-reader-macos-arm64` |

On Linux and macOS, run `chmod +x <downloaded-file>` before running it. You can rename the executable to `timeular-reader` (`timeular-reader.exe` on Windows) to match the commands below.
Linux builds target Ubuntu 22.04 and require glibc 2.35 or newer, D-Bus, and OpenSSL 3 runtime libraries (on Ubuntu: `sudo apt-get install libdbus-1-3 libssl3`).
The executables are not signed or notarized, so Windows or macOS may show a security warning.

### Publishing releases

Publishing a GitHub release (including a prerelease) automatically builds the tagged source and attaches the executables above once each build finishes. Saving a draft does not start a build.
The release tag must include `.github/workflows/release.yml`. Re-running the workflow replaces assets with matching names.
Create releases through the GitHub UI or `gh release create` using your own authentication; releases published by another workflow using `GITHUB_TOKEN` do not trigger this workflow.

## Usage

First run the command with `--setup` flag, this will generate config and let you label the sides of your device.

```console
timeular-reader --setup
```
You don't have to set up all the sides, press q on a side you don't want to use and config will generate with the ones you set up.

After the initial setup you can modify `config.toml`

### Project per side
Toggl, Clockify, Hackaru (and the Example handler) can log each side of the tracker to a different project.
During `--setup`, after the default project id, you'll be asked for a project id for every labeled side:
- leave blank to keep the current assignment (or use the default project if there is none),
- enter `-` to remove the side's assignment and fall back to the default project.

Sides without an assignment use the handler's `project_id`. You can also edit the assignments in `config.toml` under the handler's section, using the side number from the `[timeular]` section:
```toml
[[toggl.side_projects]]
side_num = 1
project_id = 123456

[[toggl.side_projects]]
side_num = 2
project_id = 654321
```
For Clockify the `project_id` is a string, e.g. `project_id = "64f1c0..."`.

To control output verbosity you can pass `--verbose` or `-v`, you can add multiple `-vvv` to make it more verbose.

There is also `--quiet`, `-q` mode to mute all output.

### Toggl
To get your project id and workspace id, on the left panel under Manage, click Projects. Then click on the project name you want to use.
The url should look like this `https://track.toggl.com/{workspace_id}/projects/{project_id}/team`

### Clockify
To generate your api key go to your profile settings on the top right. After scrolling down you'll see an option to generate API Key.

To get your workspace id, in the top right, click Your Workspace, go to Manage then settings, you should have workspace id in the url. 
It should look something like this `https://app.clockify.me/workspaces/{workspace_id}/settings`
> Note project id is optional

To get your project id on the left side, click projects, then click on your projects. The url will contain project id.
Should look something like this `https://app.clockify.me/projects/{project_id}/edit`

### Hackaru
TODO

### Traggo
TODO

## Creating your own handler
First you need to create a new mod and register it [here](https://github.com/krzysztof-ciszewski/timeular-reader/blob/ca9ff6f24c9455988dbdd89ffbd9d4c3582f636a/src/handler.rs#L13) let's call it `example`.

You create the mod by creating a file `src/handler/example.rs` and adding `pub mod example;` into the file linked above.
The `example.rs` has to have a public function called `async create_handler(setup: bool, sides: &[Side])`, and that function has to return a struct that implements [`Handler`](https://github.com/krzysztof-ciszewski/timeular-reader/blob/ca9ff6f24c9455988dbdd89ffbd9d4c3582f636a/src/tracker/config.rs#L26)
The implementation needs annotation `#[async_trait]`

It is most likely your mod will require some configuration. You can implement everything in the main `example.rs` file, but to keep it clean I recommend declaring new mod `config`.
The config mod will be responsible for creating a default config and saving it to the main config file `config.toml`.

First we need to add `pub mod config;` to `example.rs` and create file `src/handler/example/config.rs`. In `config.rs` we need to create a struct that will hold all the configuration data we will need, let's call it `ExampleConfig`.
> The derives are necessary
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleConfig {
    base_url: String,
    api_key: String,
}
```

We need to then implement `Default` and `crate::config::Config`.
The `Default` implementation can create the struct with the stuff that doesn't change much, like an API base url. For example:
```rust
impl Default for ExampleConfig {
    fn default() -> Self {
        ExampleConfig {
            base_url: String::from("https://api.example.com"),
            api_key: String::new(),
        }
    }
}
```
The `crate::config::Config` implementation can be empty, it's just about inheriting the type and a lifetime, it can look like this:
```rust
impl<'de> Config<'de> for ExampleConfig {}
```
If you want to save your config the main config file, you need to have a unique key that it will be saved under.

For convenience, you can implement methods for getting and updating the config(from/to a file). Otherwise, you will have to call `crate::config::get_config`, and `crate::config::update_config`.
These functions can look like this:
```rust
const CONFIG_KEY: &str = "example";

pub fn create_config() -> ExampleConfig {
    crate::config::get_config::<ExampleConfig>(CONFIG_KEY)
}

pub fn update_config(config: &ExampleConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
```

After that we need to register the new handler. In `src/handler.rs` you need to add our mod `Example` to the `Handlers` enum and assign it number.
```diff
pub enum Handlers {
    Toggl = 1,
    Clockify = 2,
    Traggo = 3,
    Hackaru = 4,
+   Example = 5,
}
```
then we need to adjust `TryFrom<u8>`:
```diff
fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
        x if x == Handlers::Toggl as u8 => Ok(Handlers::Toggl),
        x if x == Handlers::Clockify as u8 => Ok(Handlers::Clockify),
        x if x == Handlers::Traggo as u8 => Ok(Handlers::Traggo),
        x if x == Handlers::Hackaru as u8 => Ok(Handlers::Hackaru),
+       x if x == Handlers::Example as u8 => Ok(Handlers::Example),
        _ => Err(()),
    }
}
```
same in `TryFrom<&String>`:
```diff
fn try_from(v: &String) -> Result<Self, Self::Error> {
        match v.as_str() {
            "toggl" => Ok(Handlers::Toggl),
            "clockify" => Ok(Handlers::Clockify),
            "traggo" => Ok(Handlers::Traggo),
            "hackaru" => Ok(Handlers::Hackaru),
+           "example" => Ok(Handlers::Example),
            _ => Err(()),
        }
    }

```
The last thing to do is to adjust factory method, in `get_handler`:
```diff
pub async fn get_handler(setup: bool, config: &TimeularConfig) -> Box<dyn Handler> {
    match config.handler.as_str() {
        "toggl" => Box::new(toggl::create_handler(setup, &config.sides).await),
        "hackaru" => Box::new(hackaru::create_handler(setup, &config.sides).await),
        "clockify" => Box::new(clockify::create_handler(setup, &config.sides).await),
        "traggo" => Box::new(traggo::create_handler(setup).await),
+       "example" => Box::new(example::create_handler(setup, &config.sides).await),
        _ => Box::new(example::create_handler(setup, &config.sides).await),
    }
}
```
I have added the example tracker to the repository, you can base your module on that.

## Build
Timeular Reader requires Rust 1.78 or newer. Platform build prerequisites:

- **Windows:** Visual Studio 2022 Build Tools with the MSVC C++ toolchain and Windows 11 SDK.
- **Linux (Debian/Ubuntu):** `libdbus-1-dev` and `pkg-config`.
- **macOS:** Xcode Command Line Tools.

Then run:

```console
cargo build
```
