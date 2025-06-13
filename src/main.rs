use serialport::{SerialPortInfo, SerialPortType};
use std::time::Duration;
use std::io::{BufRead, BufReader};
use std::path::Path;
use config::{Config, File};
use serde::Deserialize;
use tokio_postgres::NoTls;
use mysql_async::prelude::*;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use tokio::time;

// ============================================================================
// Configuration structures
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DatabaseType 
{
    Postgres,
    MySQL,
    MariaDB,
}

#[derive(Debug, Deserialize)]
struct SerialConfig 
{
    port: String,
    baud_rate: u32,
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig 
{
    db_type: DatabaseType,
    host: String,
    port: u16,
    user: String,
    password: String,
    db_name: String,
    table: String,
}

#[derive(Debug, Deserialize)]
struct UploadConfig 
{
    frequency: u64,
}

#[derive(Debug, Deserialize)]
struct Settings 
{
    serial: SerialConfig,
    database: DatabaseConfig,
    upload: UploadConfig,
}

// ============================================================================
// Database abstraction
// ============================================================================

enum DatabaseInner 
{
    Postgres(tokio_postgres::Client),
    MySQL(mysql_async::Pool),
}

struct Database 
{
    inner: DatabaseInner,
    table_name: String,
}

impl Database 
{
    /// Créer une nouvelle connexion à la base de données
    async fn new(config: &DatabaseConfig) -> Result<Self, Box<dyn std::error::Error>> 
    {
        let inner = match config.db_type 
        {
            DatabaseType::Postgres => Self::connect_postgres(config).await?,
            DatabaseType::MySQL | DatabaseType::MariaDB => Self::connect_mysql(config).await?,
        };

        let db = Database 
        {
            inner,
            table_name: config.table.clone(),
        };

        db.create_table_if_not_exists().await?;
        Ok(db)
    }

    /// Connexion PostgreSQL
    async fn connect_postgres(config: &DatabaseConfig) -> Result<DatabaseInner, Box<dyn std::error::Error>> 
    {
        let connection_string = format!(
            "host={} port={} user={} password={} dbname={}",
            config.host, config.port, config.user, config.password, config.db_name
        );

        let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;

        // Gérer la connexion en arrière-plan
        tokio::spawn(async move 
        {
            if let Err(e) = connection.await 
            {
                eprintln!("Erreur de connexion PostgreSQL: {}", e);
            }
        });

        Ok(DatabaseInner::Postgres(client))
    }

    /// Connexion MySQL/MariaDB
    async fn connect_mysql(config: &DatabaseConfig) -> Result<DatabaseInner, Box<dyn std::error::Error>> 
    {
        let url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.user, config.password, config.host, config.port, config.db_name
        );
        
        let pool = mysql_async::Pool::new(url.as_str());
        Ok(DatabaseInner::MySQL(pool))
    }

    /// Créer la table si elle n'existe pas
    async fn create_table_if_not_exists(&self) -> Result<(), Box<dyn std::error::Error>> 
    {
        match &self.inner 
        {
            DatabaseInner::Postgres(client) => 
            {
                let query = format!(
                    "CREATE TABLE IF NOT EXISTS {} (
                        id SERIAL PRIMARY KEY,
                        timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
                        value TEXT NOT NULL
                    )",
                    self.table_name
                );
                client.execute(&query, &[]).await?;
            }
            DatabaseInner::MySQL(pool) => 
            {
                let query = format!(
                    "CREATE TABLE IF NOT EXISTS {} (
                        id INT AUTO_INCREMENT PRIMARY KEY,
                        timestamp TIMESTAMP NOT NULL,
                        value TEXT NOT NULL
                    )",
                    self.table_name
                );
                let mut conn = pool.get_conn().await?;
                conn.query_drop(query).await?;
            }
        }
        Ok(())
    }

    /// Insérer une valeur dans la base de données
    async fn insert_value(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> 
    {
        let now: DateTime<Utc> = Utc::now();
        
        match &self.inner 
        {
            DatabaseInner::Postgres(client) => 
            {
                let query = format!(
                    "INSERT INTO {} (timestamp, value) VALUES ($1, $2)",
                    self.table_name
                );
                client.execute(&query, &[&now, &value]).await?;
            }
            DatabaseInner::MySQL(pool) => 
            {
                let query = format!(
                    "INSERT INTO {} (timestamp, value) VALUES (?, ?)",
                    self.table_name
                );
                let mut conn = pool.get_conn().await?;
                let mysql_timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                conn.exec_drop(query, (mysql_timestamp, value)).await?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Serial port utilities
// ============================================================================

struct SerialPortManager;

impl SerialPortManager 
{
    /// Lister tous les ports série disponibles
    fn list_available_ports() -> Result<Vec<SerialPortInfo>, Box<dyn std::error::Error>> 
    {
        let ports = serialport::available_ports()?;
        Ok(ports)
    }

    /// Afficher les ports série disponibles
    fn display_available_ports() 
    {
        println!("Ports série disponibles :");
        
        match Self::list_available_ports() 
        {
            Ok(ports) => 
            {
                for port in ports 
                {
                    match port.port_type 
                    {
                        SerialPortType::UsbPort(info) => 
                        {
                            println!("  USB - {} ({})", 
                                port.port_name, 
                                info.product.unwrap_or_default()
                            );
                        }
                        SerialPortType::PciPort => 
                        {
                            println!("  PCI - {}", port.port_name);
                        }
                        SerialPortType::BluetoothPort => 
                        {
                            println!("  Bluetooth - {}", port.port_name);
                        }
                        SerialPortType::Unknown => 
                        {
                            println!("  Inconnu - {}", port.port_name);
                        }
                    }
                }
            }
            Err(e) => 
            {
                eprintln!("Erreur lors de la recherche des ports série: {}", e);
            }
        }
    }

    /// Ouvrir un port série avec la configuration donnée
    fn open_port(config: &SerialConfig) -> Result<Box<dyn serialport::SerialPort>, Box<dyn std::error::Error>> 
    {
        let port = serialport::new(&config.port, config.baud_rate)
            .timeout(Duration::from_millis(config.timeout_ms))
            .open()?;
        
        Ok(port)
    }
}

// ============================================================================
// Configuration management
// ============================================================================

struct ConfigManager;

impl ConfigManager 
{
    /// Charger la configuration depuis le fichier TOML
    fn load() -> Result<Settings, Box<dyn std::error::Error>> 
    {
        let config_path = Path::new("config/default.toml");
        
        let settings = Config::builder()
            .add_source(File::from(config_path))
            .build()?;

        let settings = settings.try_deserialize()?;
        Ok(settings)
    }

    /// Afficher la configuration actuelle
    fn display(settings: &Settings) 
    {
        println!("\nConfiguration actuelle :");
        println!("  Port : {}", settings.serial.port);
        println!("  Baud rate : {}", settings.serial.baud_rate);
        println!("  Timeout : {} ms", settings.serial.timeout_ms);
        println!("  Fréquence d'upload : {} secondes", settings.upload.frequency);
        println!("  Base de données : {:?}", settings.database.db_type);
        println!("  Table : {}", settings.database.table);
    }
}

// ============================================================================
// Data processor
// ============================================================================

struct DataProcessor 
{
    last_value: Arc<Mutex<Option<String>>>,
}

impl DataProcessor 
{
    fn new() -> Self 
    {
        Self 
        {
            last_value: Arc::new(Mutex::new(None)),
        }
    }

    /// Traiter une nouvelle ligne reçue
    fn process_line(&self, line: &str) 
    {
        let trimmed_line = line.trim();
        
        if !trimmed_line.is_empty() 
        {
            println!("Ligne reçue: {}", trimmed_line);
            
            // Mettre à jour la dernière valeur seulement si elle a changé
            let mut last = self.last_value.lock().unwrap();
            if last.as_ref() != Some(&trimmed_line.to_string()) 
            {
                *last = Some(trimmed_line.to_string());
            }
        }
    }

    /// Démarrer la tâche d'upload périodique
    async fn start_upload_task(&self, database: Database, upload_frequency: u64) 
    {
        let last_value_clone = Arc::clone(&self.last_value);
        
        tokio::spawn(async move 
        {
            let mut interval = time::interval(Duration::from_secs(upload_frequency));
            
            loop 
            {
                interval.tick().await;
                
                let value = {
                    let mut guard = last_value_clone.lock().unwrap();
                    guard.take()
                };
                
                if let Some(val) = value 
                {
                    match database.insert_value(&val).await 
                    {
                        Ok(_) => 
                        {
                            println!("✓ Valeur uploadée avec succès: {}", val);
                        }
                        Err(e) => 
                        {
                            eprintln!("✗ Erreur lors de l'upload: {}", e);
                        }
                    }
                }
            }
        });
    }
}

// ============================================================================
// Serial reader
// ============================================================================

struct SerialReader;

impl SerialReader 
{
    /// Lire les données du port série de manière continue
    async fn read_continuous(
        port: Box<dyn serialport::SerialPort>, 
        processor: &DataProcessor
    ) -> Result<(), Box<dyn std::error::Error>> 
    {
        let reader = BufReader::new(port);
        let mut lines = reader.lines();
        
        println!("\n🔄 Lecture des données du port série...");
        
        loop 
        {
            match lines.next() 
            {
                Some(Ok(line)) => 
                {
                    processor.process_line(&line);
                }
                Some(Err(e)) => 
                {
                    if e.kind() == std::io::ErrorKind::TimedOut 
                    {
                        // En cas de timeout, on continue
                        continue;
                    }
                    eprintln!("Erreur lors de la lecture du port série: {}", e);
                    break;
                }
                None => 
                {
                    // Attendre un peu si aucune ligne n'est disponible
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        }
        
        Ok(())
    }
}

// ============================================================================
// Application main
// ============================================================================

struct Application;

impl Application 
{
    /// Point d'entrée principal de l'application
    async fn run() -> Result<(), Box<dyn std::error::Error>> 
    {
        println!("🚀 Démarrage de l'application de lecture série");
        
        // Charger la configuration
        let settings = ConfigManager::load()?;
        
        // Afficher les informations système
        SerialPortManager::display_available_ports();
        ConfigManager::display(&settings);
        
        // Ouvrir le port série
        let port = SerialPortManager::open_port(&settings.serial)?;
        println!("\n✓ Port série ouvert avec succès");
        
        // Initialiser la connexion à la base de données
        let database = Database::new(&settings.database).await?;
        println!("✓ Connexion à la base de données établie");
        
        // Initialiser le processeur de données
        let processor = DataProcessor::new();
        
        // Démarrer la tâche d'upload
        processor.start_upload_task(database, settings.upload.frequency).await;
        println!("✓ Tâche d'upload démarrée");
        
        // Commencer la lecture série
        SerialReader::read_continuous(port, &processor).await?;
        
        Ok(())
    }
}

// ============================================================================
// Main function
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> 
{
    if let Err(e) = Application::run().await 
    {
        eprintln!("❌ Erreur fatale: {}", e);
        std::process::exit(1);
    }
    
    Ok(())
}