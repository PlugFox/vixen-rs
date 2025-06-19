use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments - this will automatically handle --help
    let config = Config::parse();

    println!("Starting Telegram Bot Server...");
    println!("Configuration:");
    println!("  Address: {}", config.address);
    println!("  Port: {}", config.port);
    println!("  Log Level: {}", config.log_level);

    // TODO: Implement actual server logic here
    println!("Server would start at {}:{}", config.address, config.port);

    Ok(())
}

#[derive(Parser, Debug)]
#[command(
    name = "vixen",
    version,
    about = "Telegram Bot Server for automatically banning spammers",
    long_about = "A Telegram bot server that automatically detects and bans spammers in Telegram chats. \
                  This server provides a REST API and connects to the Telegram Bot API to monitor \
                  chat messages and take action against spam accounts."
)]
pub struct Config {
    /// Address to bind the server to (CLI > ENV > default)
    #[arg(
        short = 'a',
        long,
        env = "ADDRESS",
        default_value = "0.0.0.0",
        aliases = ["host", "addr"],
        help = "IP address to bind the server to"
    )]
    address: String,

    /// Port to bind the server to (CLI > ENV > default)
    #[arg(
        short = 'p',
        long = "port",
        env = "PORT",
        default_value_t = 8080,
        help = "Port number to bind the server to"
    )]
    port: u16,

    /// Level of logging (CLI > ENV > default)
    /// This can be set to trace, debug, info, warn, or error
    #[arg(
        short = 'l',
        long = "logs",
        env = "LOGS",
        default_value = "info",
        aliases = ["log", "level", "verbose"],
        help = "Logging level (trace, debug, info, warn, error)"
    )]
    log_level: String,
}