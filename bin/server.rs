use vixen::ui::app::App;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?; // Install color_eyre for error handling
    let terminal = ratatui::init(); // Initialize the terminal
    let result = App::new().run(terminal).await; // Run the application
    ratatui::restore(); // Restore the terminal state
    result // Return the result of the application run
}
