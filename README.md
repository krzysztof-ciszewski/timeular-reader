<h1 align="center">Timeular Reader</h1>
<p align="center">Have you bought the expensive <a href="https://timeular.com/tracker">Timeular tracker</a> and don't want to pay on top of that for their propriatery app? This project is for you. With Timeular Reader you can connnect your tracker to your favourite time tracking app.
</p>

## Usage

First run the command with `--setup` flag, this will generate config and let you label the sides of your device.

```console
timeular-reader --setup
```
You don't have to set up all the sides, press q on a side you don't want to use and config will generate with the ones you set up.

After the initial setup you can modify `config.toml`

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
The `example.rs` has to have a public function called `async create_handler(setup: bool)`, and that function has to return a struct that implements [`Handler`](https://github.com/krzysztof-ciszewski/timeular-reader/blob/ca9ff6f24c9455988dbdd89ffbd9d4c3582f636a/src/tracker/config.rs#L26)
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
Then in CreateHandler, the example key has to accept the implementation of the `Handler`:
```diff
pub enum CreateHandler {
    #[default]
    None,
    Toggl(Toggl),
    Clockify(Clockify),
    Traggo(Traggo),
    Hackaru(Hackaru),
+   Example(crate::handler::example::Example)
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
    match v {
        x if x.to_lowercase() == format!("{:?}", Handlers::Toggl).to_lowercase() => {
            Ok(Handlers::Toggl)
        }
        x if x.to_lowercase() == format!("{:?}", Handlers::Clockify).to_lowercase() => {
            Ok(Handlers::Clockify)
        }
        x if x.to_lowercase() == format!("{:?}", Handlers::Traggo).to_lowercase() => {
            Ok(Handlers::Traggo)
        }
        x if x.to_lowercase() == format!("{:?}", Handlers::Hackaru).to_lowercase() => {
            Ok(Handlers::Hackaru)
        }
+       x if x.to_lowercase() == format!("{:?}", Handlers::Example).to_lowercase() => {
+           Ok(Handlers::Example)
+       }
        _ => Err(()),
    }
}
```
then in `get_create_handler`:
```diff
return match handler {
    Handlers::Toggl => CreateHandler::Toggl(toggl::create_handler(setup).await),
    Handlers::Clockify => CreateHandler::Clockify(clockify::create_handler(setup).await),
    Handlers::Traggo => CreateHandler::Traggo(traggo::create_handler(setup).await),
    Handlers::Hackaru => CreateHandler::Hackaru(hackaru::create_handler(setup).await),
+   Handlers::Example => CreateHandler::Example(example::create_handler(setup).await),
};
```
and lastly `handle`:
```diff
    match create_handler {
        CreateHandler::Toggl(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Clockify(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Traggo(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Hackaru(h) => {
            h.handle(side, duration).await;
        }
+       CreateHandler::Example(h) => {
+           h.handle(side, duration).await;
+       }
        CreateHandler::None => {
            panic!("CreateHandler should never be none")
        }
    }
```
I have added the example tracker to the repository, you can base your module on that.

## Build
Simply run
```console
cargo build
```
