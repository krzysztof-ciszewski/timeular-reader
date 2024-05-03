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

### Toggl
To get your project id and workspace id, on the left panel under Manage, click Projects. Then click on the project name you want to use.
The url should look like this `https://track.toggl.com/{workspace_id}/projects/{project_id}/team`

### Clockify

### Hackaru

### Traggo

## Creating your own handler

## Build
Simply run
```console
cargo build
```
