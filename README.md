# G-optimizer

TSP G-code optimizer is a program to optimize 3D printing g-code using Traveling Salesman Problem heuristics, mainly Lin-Kernighan Heuristic.

The program has been tested with the following slicers :
- PrusaSlicer 2.6.1
- Cura_SteamEngine 15.01
- Slic3r 1.3.1-dev

In case a command isn't treated by the program, it will be displayed in the console and will be logged into the `logs/` folder.
Please report that/those command/s in an issue so they can be treated or treat them and make a pull request.

## How to use

On windows, download G-optimizer.exe from release tags, then download an .exe of LKH from the following link : http://webhotel4.ruc.dk/~keld/research/LKH/

Create a config file (extension doesn't matter, the file contains a JSON structure so you could use a .json) with the following values :
```JSON
{
    "program": "./LKH-2.exe",
    "precision": 1000,
    "num_runs": 1,
    "max_merge_length": 10.000
}
```

Make sure the program value points to your LKH executable.

In a console (CMD or Powershell), use the following command to run the application :
```
.\G-optimizer.exe your_config.json your_g-code_file.gcode
```

For MacOS and Linux, refer to the next section.

## How to install and run the source code

To build LKH, you'll need `make` to be able to generate the executable.

On Ubuntu, you can install `make` using the following command : 
```
apt-get install make
```

On MacOS, install brew following the instructions on its homepage : https://brew.sh/

Then, install `make` using the following command :
```
brew install make
```

Download the source code `LKH-2.0.10.tgz` from the following page : http://webhotel4.ruc.dk/~keld/research/LKH/ and execute the necessary commands to generate an executable file called `LKH`, you'll need to reference that file in your configuration so move that file wherever you please for ease of access.

For G-optimizer, you'll need `Cargo` to be able to run Rust code.

On Ubuntu, you can install `Cargo` using the following command :
```
apt-get install cargo
```

On MacOS, you can install `Cargo` using the following command (Replace `<version>` with the version you find in that folder) :
```
brew install rustup-init
cd /opt/Homebrew/Cellar/rustup-init/<version>/bin
./rustup-init
```

Select your wanted installation parameters and once the installation is done, run the following command to verify it works :
```
rustc --version
```

If that command doesn't work, you need to configure your `PATH` by following rustup-init's instructions which should have been provided at the end of the installation.

Download the source code and then create a config file with the following content (Extension doesn't matter) :
```JSON
{
    "program": "./LKH",
    "precision": 1000,
    "num_runs": 1,
    "max_merge_length": 10.000
}
```

Make sure that the program value points to your LKH executable generated previously.

You can then run the program using the following command :
```
cargo run your_config.json your_g-code_file.gcode
```

If you prefer an executable, you can use `cargo build --release` and get your executable from the `target/release/` folder. In that case you only need this executable, your LKH executable and the config file. You can put all of them wherever you'd like.

## Problems when running the program

The program might crash with this kind of message :
```
thread `main` panicked at app.rs:...
called `Option::unwrap()` on a `None` value
```

Your OS should also display a message which indicates the actual problem. This could include messages like "Too many open files". "Too many open files" can potentialy be corrected on macOS using `ulimit -n <num>`. Replace `<num>` with the amount of files you allow to be opened at once, 2048 should be enough. 

If your can't find a solution, please create an issue reporting that problem and provide the OS's message and which OS you're using.