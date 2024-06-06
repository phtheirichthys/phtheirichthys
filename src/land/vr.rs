use std::path::{Path, PathBuf};
use anyhow::{bail, Result};
use crate::land::LandsProvider;

pub(crate) struct VrLandProvider {
    tiles: Box<[[Tile;360];180]>,
}

impl LandsProvider for VrLandProvider {

    fn is_land(&self, lat: f64, lon: f64) -> bool {
        let tile_lat = lat.ceil() as i32;
        let tile_lon = lon.floor() as i32;

        let d_lat = tile_lat - Self::LAT_0;
        let mut d_lon = tile_lon - Self::LON_0;

        if d_lat < 0 || d_lat >= Self::LAT_N {
            return false
        }

        while d_lon < 0 {
            d_lon += Self::LON_N;
        }
        while d_lon >= Self::LON_N {
            d_lon -= Self::LON_N;
        }

        match &self.tiles[d_lat as usize][d_lon as usize] {
            Tile::Sea => false,
            Tile::Mixed(tile) => {
                let d_lat = ((tile_lat as f64 - lat) * 730.0) as usize;
                let d_lon = ((lon - tile_lon as f64) * 730.0) as usize;

                let p = d_lat * 730 + d_lon;

                tile[p/8] >> (7 - p%8) & 0x01 == 0x01
            }
            Tile::Land => true,
        }
    }

    fn near_land(&self, lat: f64, lon: f64) -> bool {

        let (mut sea, mut mixed, mut land) = (false, false, false);

        for i in -1..2 {
            for j in -1..2 {
                let tile_lat = lat.ceil() as i32 + i;
                let tile_lon = lon.floor() as i32 + j;

                let d_lat = tile_lat - Self::LAT_0;
                let mut d_lon = tile_lon - Self::LON_0;

                if d_lat < 0 || d_lat >= Self::LAT_N {
                    continue
                }

                while d_lon < 0 {
                    d_lon += Self::LON_N;
                }
                while d_lon >= Self::LON_N {
                    d_lon -= Self::LON_N;
                }

                match &self.tiles[d_lat as usize][d_lon as usize] {
                    Tile::Sea => { sea = true },
                    Tile::Mixed(_) => { mixed = true }
                    Tile::Land => { land = true },
                }
            }
        }

        if mixed || sea && land {
            for i in -5..6 {
                for j in -5..6 {
                    let lat = lat + (i as f64) / 730.0;
                    let lon = lon + (j as f64) / 730.0;

                    if self.is_land(lat, lon) {
                        return true
                    }
                }
            }

            return false
        }

        land
    }
}

impl VrLandProvider {
    const LAT_0: i32 = -89;
    const LAT_N: i32 = 180;
    const LON_0: i32 = -180;
    const LON_N: i32 = 360;

    pub(crate) fn _new(tiles_dir: &String) -> Result<Self> {

        let index = match std::fs::read(PathBuf::from(tiles_dir).join("index")) {
            Ok(index) => index,
            Err(e) => {
                bail!("Error loading tiles index : {}", e);
            }
        };

        const LAND: Tile = Tile::Land;
        const LAND_ARRAY: [Tile;360] = [LAND;360];

        let mut tiles_array: Box<[[Tile;360];180]> = Box::new([LAND_ARRAY;180]);

        for d_lat in 0..180 {
            let latitude = Self::LAT_0 + d_lat as i32;

            for d_lon in 0..360 {
                let longitude = Self::LON_0 + d_lon as i32;

                let file_name = format!("1_{}_{}.deg", longitude, latitude);

                let p = d_lat * Self::LON_N as usize + d_lon;

                let tile = match (index[p/4] >> (6 - 2*(p%4))) & 3 {
                    0 => Tile::Sea,
                    1 => Tile::load(PathBuf::from(tiles_dir).join(&"carto").join(file_name).as_path())?,
                    2 => Tile::Land,
                    _ => {
                        bail!("bad value");
                    }
                };

                tiles_array[d_lat][d_lon] = tile;
            }
        }

        Ok(Self {
            tiles: tiles_array,
        })
    }
}

#[derive(Default)]
enum Tile {
    Sea,
    #[default]
    Land,
    Mixed(Vec<u8>),
}

impl Tile {
    fn load(file_name: &Path) -> Result<Tile> {

        let buf = match std::fs::read(file_name) {
            Ok(buf) => buf,
            Err(e) => {
                bail!("Error loading tile {:?} : {}", file_name, e);
            }
        };

        Ok(Tile::Mixed(buf))
    }
}
