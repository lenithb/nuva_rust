//! clientes http para vista sugeridos: radios (radio-browser.info) y
//! temas de archive.org. ambos son gratuitos y no piden clave api; el
//! detalle de cada servicio queda en los comentarios de cada rutina.

use std::io::Read;
use std::sync::mpsc;
use std::thread;

/// encabezado de identificacion de la app: radio-browser lo pide por
/// cortesía. se toma una vez y se reutiliza en cada pedido.
pub fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("nuva/0.2 (reproductor libre)")
        .build()
        .map_err(|e| format!("no se pudo crear el cliente http: {e}"))
}

// ── radio-browser.info ───────────────────────────────────────────────

/// estacion de radio: nombre y stream directo (normalmente mp3).
#[derive(Clone, Debug)]
pub struct Estacion {
    pub nombre: String,
    pub url: String,
    pub bitrate: u32,
    pub codec: String,
}

/// consulta la lista de radios; `consulta` filtra por nombre
/// ("lofi", "rock", etc.) y `limite` acota el resultado.
pub fn buscar_radios(consulta: &str, limite: usize) -> Result<Vec<Estacion>, String> {
    let url = format!(
        "https://all.api.radio-browser.info/json/stations/search?name={consulta}&limit={limite}&hidebroken=true&order=clickcount&reverse=true"
    );
    let json = get_json(&url)?;
    let arr = json
        .as_array()
        .ok_or("radio-browser no devolvio un array")?;
    let mut radios = Vec::new();
    for item in arr {
        let obj = item.as_object().ok_or("entrada invalida en la lista de radios")?;
        radios.push(Estacion {
            nombre: str_of(obj, "name"),
            url: str_of(obj, "url_resolved"),
            bitrate: obj.get("bitrate").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            codec: str_of(obj, "codec"),
        });
    }
    if radios.is_empty() {
        return Err(format!("sin resultados para «{consulta}»"));
    }
    Ok(radios)
}

// ── archive.org ──────────────────────────────────────────────────────

/// tema de archive.org: coleccion creative commons / dominio publico.
/// la url mp3 se resuelve aparte (pide el manifesto del item).
#[derive(Clone, Debug)]
pub struct Tema {
    pub id: String,
    pub titulo: String,
    /// url del mp3, resuelta al momento de reproducir.
    pub url: Option<String>,
}

/// busca items de audio en archive.org; solo resultados con mp3.
/// el índice responde con `{response: {doc: [...]}}`; de ahí se saca
/// la lista de documentos.
pub fn buscar_temas(consulta: &str, limite: usize) -> Result<Vec<Tema>, String> {
    let q = format!(
        "https://archive.org/advancedsearch.php?q={consulta}%20AND%20format%3AMP3&fl[]=identifier&fl[]=title&rows={limite}&output=json"
    );
    let json = get_json(&q)?;
    let docs = json
        .get("response")
        .and_then(|r| r.get("docs"))
        .and_then(|d| d.as_array())
        .ok_or("respuesta de archive.org sin documentos")?;
    Ok(docs
        .iter()
        .filter_map(|doc| {
            let obj = doc.as_object()?;
            let id = str_of(obj, "identifier");
            if id.is_empty() {
                return None;
            }
            Some(Tema { id, titulo: str_of(obj, "title"), url: None })
        })
        .collect())
}

/// resuelve la url del primer mp3 del item archive.org de id dado.
pub fn resolver_tema(id: &str) -> Result<String, String> {
    let json = get_json(&format!("https://archive.org/metadata/{id}"))?;
    let files = json
        .get("files")
        .and_then(|f| f.as_array())
        .ok_or("manifesto de archive.org incompleto")?;
    for file in files {
        let obj = file.as_object().ok_or("archivo invalido")?;
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        if name.to_lowercase().ends_with(".mp3") {
            return Ok(format!(
                "https://archive.org/download/{}/{}",
                id,
                name.replace(' ', "%20")
            ));
        }
    }
    Err(format!("el item {id} no tiene archivos mp3"))
}

// ── comunes ──────────────────────────────────────────────────────────

/// pedido get que devuelve json; errores con mensaje humano.
fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let text = client()?
        .get(url)
        .send()
        .and_then(|resp| resp.error_for_status())
        .and_then(|resp| resp.text())
        .map_err(|e| format!("fallo de red para {url}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("json invalido de {url}: {e}"))
}

/// campo string con valor por defecto (evita choques con el tipo).
fn str_of(obj: &serde_json::Map<String, serde_json::Value>, campo: &str) -> String {
    obj.get(campo).and_then(|v| v.as_str()).unwrap_or_default().to_owned()
}

// ── stream a rodio ───────────────────────────────────────────────────
//
// rodio decodifica de una fuente Io; para no bloquear su hilo de audio
// con la red se descarga en un hilo aparte que empuja bloques por un
// canal, y el lector solo vacia el canal. patron basico de rodio para
// streams http (radio mp3 de por vida incluida).

/// lector que recibe bloques de red por canal y los sirve como `Read`.
/// `None` en el canal significa fin del stream (con o sin error).
/// el mutex hace el tipo sync, que rodio exige para mover la fuente a
/// su hilo de audio.
pub struct HttpReader {
    rx: std::sync::Mutex<mpsc::Receiver<Option<Vec<u8>>>>,
    buffer: Vec<u8>,
    /// bytes logicos servidos hasta ahora (para el seek ficticio).
    pos: u64,
}

impl Read for HttpReader {
    fn read(&mut self, salida: &mut [u8]) -> std::io::Result<usize> {
        let rx = self.rx.lock().expect("lock del stream");
        while self.buffer.is_empty() {
            match rx.recv() {
                Ok(Some(bloque)) => self.buffer = bloque,
                Ok(None) | Err(_) => return Ok(0), // fin del stream
            }
        }
        let n = salida.len().min(self.buffer.len());
        salida[..n].copy_from_slice(&self.buffer[..n]);
        self.buffer.drain(..n);
        self.pos += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for HttpReader {
    /// el stream no es seekable de verdad: rodio/symphonia declaran
    /// `is_seekable = false` y no deberian pedir un seek real; este
    /// stub satisface el bound del tipo devolviendo la posicion leida.
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Ok(self.pos)
    }
}

/// vuelca la respuesta http al canal en bloques; `None` marca el final.
fn bombear(resp: reqwest::blocking::Response, tx: mpsc::Sender<Option<Vec<u8>>>) {
    let mut resp = resp;
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match resp.read(&mut buf) {
            Ok(0) => break, // fin del stream
            Ok(n) => {
                if tx.send(Some(buf[..n].to_vec())).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = tx.send(None);
}

/// decodificador conectado a una url (radio o archivo remoto).
/// como puede fallar en cualquier momento del stream, los errores
/// terminan la pista; la vista decide que mostrar.
pub fn decodificador_de(
    url: &str,
) -> Result<rodio::Decoder<std::io::BufReader<HttpReader>>, String> {
    let url = url.to_owned();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let resp = client().and_then(|c| {
            c.get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.to_string())
        });
        match resp {
            Ok(resp) => bombear(resp, tx),
            Err(_) => {
                let _ = tx.send(None);
            }
        }
    });
    rodio::Decoder::new_mp3(std::io::BufReader::new(HttpReader {
        rx: std::sync::Mutex::new(rx),
        buffer: Vec::new(),
        pos: 0,
    }))
    .map_err(|e| format!("no se pudo decodificar el stream: {e}"))
}
