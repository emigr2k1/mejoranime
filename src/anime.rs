use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anime {
    pub titulo: String,
    pub sinopsis: String,
    pub puntuacion: f32,
    pub fecha_lanzamiento: String,
    pub tipo: String,
    pub portada: String,
    pub estado: String,
    pub generos: Vec<String>,
    pub episodios: Vec<Episodio>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episodio {
    pub numero: f32,
    pub servidores: Vec<Servidor>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Servidor {
    pub nombre: String,
    pub url: String,
}
