//! Deterministic supply-chain seed — the same world the TS workbench ships
//! with: 9 object types, 10 link types, 3 action types, 66 objects and ~92
//! instance links. Port of `server/seed.ts`; ids are PRNG-derived so every
//! fresh store is byte-identical (handy for tests and demos).

use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};

use crate::model::*;
use crate::store::{State, Store};

/// mulberry32 — the same PRNG the TS seed uses.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as u64 as f64) / 4294967296.0
    }
    fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        &arr[(self.next() * arr.len() as f64) as usize % arr.len()]
    }
    fn int(&mut self, min: i64, max: i64) -> i64 {
        (self.next() * (max - min + 1) as f64) as i64 + min
    }
    fn dec(&mut self, min: f64, max: f64) -> f64 {
        (min + self.next() * (max - min) * 100.0).round() / 100.0
    }
    /// pseudo-UUID — deterministic because the stream is.
    fn uuid(&mut self) -> String {
        let mut b = [0u8; 16];
        for chunk in b.chunks_mut(4) {
            chunk.copy_from_slice(&((self.next() * u32::MAX as f64) as u32).to_le_bytes());
        }
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15],
        )
    }
}

fn prop(api: &str, display: &str, data_type: DataType, required: bool) -> PropertyDef {
    PropertyDef {
        api_name: api.into(),
        display_name: display.into(),
        data_type,
        required,
        array: false,
    }
}

const C: [&str; 8] = [
    "#f0b35c", // amber
    "#7da7d9", // sky
    "#6bbfae", // teal
    "#a88fd6", // violet
    "#d98a94", // rose
    "#a9c47f", // lime
    "#e0925a", // orange
    "#c689c1", // magenta
];

fn obj(rng: &mut Rng, type_id: &str, title: String, properties: BTreeMap<String, Value>) -> Obj {
    Obj {
        id: rng.uuid(),
        type_id: type_id.into(),
        title,
        properties,
        created_at: "2026-11-21T00:00:00Z".into(),
    }
}

fn props(pairs: Vec<(&str, Value)>) -> BTreeMap<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// The seeded store — call it, don't configure it.
pub fn supply_chain() -> Store {
    let mut rng = Rng(0x0171c);

    // ---- object types ----------------------------------------------------
    let defs = [
        (
            "Port",
            "Port",
            "Ports",
            "Location",
            C[2],
            "A marine terminal where vessels berth and cargo is exchanged.",
            vec![
                prop("port_code", "Port code", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("country", "Country", DataType::String, false),
                prop("latitude", "Latitude", DataType::Decimal, false),
                prop("longitude", "Longitude", DataType::Decimal, false),
                prop("berth_count", "Berths", DataType::Integer, false),
            ],
        ),
        (
            "Vessel",
            "Vessel",
            "Vessels",
            "Maritime",
            C[1],
            "An oceangoing cargo vessel tracked across the network.",
            vec![
                prop("imo_number", "IMO number", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("flag", "Flag state", DataType::String, false),
                prop("capacity_teu", "Capacity (TEU)", DataType::Integer, false),
                prop("status", "Status", DataType::String, false),
                prop("current_port_code", "Current port", DataType::String, false),
            ],
        ),
        (
            "Shipment",
            "Shipment",
            "Shipments",
            "Logistics",
            C[0],
            "A booked movement of cargo between two ports.",
            vec![
                prop("shipment_id", "Shipment ID", DataType::String, true),
                prop("status", "Status", DataType::String, true),
                prop("teu", "Volume (TEU)", DataType::Integer, false),
                prop("etd", "ETD", DataType::Date, false),
                prop("eta", "ETA", DataType::Date, false),
            ],
        ),
        (
            "Container",
            "Container",
            "Containers",
            "Logistics",
            C[6],
            "An intermodal container assigned to shipments.",
            vec![
                prop(
                    "container_number",
                    "Container number",
                    DataType::String,
                    true,
                ),
                prop("size", "Size", DataType::String, false),
                prop("reefer", "Reefer", DataType::Boolean, false),
                prop("seal_number", "Seal number", DataType::String, false),
            ],
        ),
        (
            "Product",
            "Product",
            "Products",
            "Catalog",
            C[3],
            "A sellable good in the catalog.",
            vec![
                prop("sku", "SKU", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("category", "Category", DataType::String, false),
                prop(
                    "unit_weight_kg",
                    "Unit weight (kg)",
                    DataType::Decimal,
                    false,
                ),
                prop("hazmat", "Hazmat", DataType::Boolean, false),
            ],
        ),
        (
            "Factory",
            "Factory",
            "Factories",
            "Facility",
            C[5],
            "A manufacturing site producing catalog products.",
            vec![
                prop("factory_code", "Factory code", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("city", "City", DataType::String, false),
                prop("country", "Country", DataType::String, false),
                prop("lines", "Production lines", DataType::Integer, false),
                prop(
                    "annual_capacity",
                    "Annual capacity",
                    DataType::Integer,
                    false,
                ),
            ],
        ),
        (
            "Supplier",
            "Supplier",
            "Suppliers",
            "Organization",
            C[4],
            "An external organization supplying factories.",
            vec![
                prop("tax_id", "Tax ID", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("tier", "Tier", DataType::Integer, false),
                prop("country", "Country", DataType::String, false),
                prop("risk_score", "Risk score", DataType::Decimal, false),
            ],
        ),
        (
            "Customer",
            "Customer",
            "Customers",
            "Organization",
            C[4],
            "An account that places sales orders.",
            vec![
                prop("account_id", "Account ID", DataType::String, true),
                prop("name", "Name", DataType::String, true),
                prop("segment", "Segment", DataType::String, false),
                prop("region", "Region", DataType::String, false),
            ],
        ),
        (
            "SalesOrder",
            "Sales order",
            "Sales orders",
            "Commerce",
            C[7],
            "A commercial order placed by a customer.",
            vec![
                prop("order_number", "Order number", DataType::String, true),
                prop("channel", "Channel", DataType::String, false),
                prop("placed_at", "Placed at", DataType::Timestamp, false),
                prop("total", "Total", DataType::Decimal, false),
                prop("currency", "Currency", DataType::String, false),
            ],
        ),
    ];

    let mut types: Vec<ObjectType> = defs
        .iter()
        .map(
            |(api, display, plural, domain, color, desc, properties)| ObjectType {
                id: rng.uuid(),
                api_name: api.to_string(),
                display_name: display.to_string(),
                plural_display_name: plural.to_string(),
                description: desc.to_string(),
                domain: domain.to_string(),
                color: color.to_string(),
                properties: properties.clone(),
            },
        )
        .collect();
    // insertion order mirrors the TS seed (sorted by domain on read there;
    // the store keeps insertion order here — exports stay deterministic)
    let _ = &mut types;
    let by_api = |api: &str| types.iter().find(|t| t.api_name == api).unwrap();

    // ---- link types ------------------------------------------------------
    let link_defs: [(&str, &str, &str, &str, Cardinality, Cardinality); 10] = [
        (
            "CURRENT_PORT",
            "Current port",
            "Vessel",
            "Port",
            Cardinality::Many,
            Cardinality::One,
        ),
        (
            "DEPARTS_FROM",
            "Departs from",
            "Shipment",
            "Port",
            Cardinality::Many,
            Cardinality::One,
        ),
        (
            "ARRIVES_AT",
            "Arrives at",
            "Shipment",
            "Port",
            Cardinality::Many,
            Cardinality::One,
        ),
        (
            "CARRIED_BY",
            "Carried by",
            "Shipment",
            "Vessel",
            Cardinality::Many,
            Cardinality::One,
        ),
        (
            "PART_OF_SHIPMENT",
            "Part of shipment",
            "Container",
            "Shipment",
            Cardinality::Many,
            Cardinality::One,
        ),
        (
            "HOLDS_PRODUCT",
            "Holds product",
            "Container",
            "Product",
            Cardinality::Many,
            Cardinality::Many,
        ),
        (
            "PRODUCES",
            "Produces",
            "Factory",
            "Product",
            Cardinality::Many,
            Cardinality::Many,
        ),
        (
            "SUPPLIES",
            "Supplies",
            "Supplier",
            "Factory",
            Cardinality::Many,
            Cardinality::Many,
        ),
        (
            "CONTAINS_PRODUCT",
            "Contains product",
            "SalesOrder",
            "Product",
            Cardinality::Many,
            Cardinality::Many,
        ),
        (
            "PLACED_BY",
            "Placed by",
            "SalesOrder",
            "Customer",
            Cardinality::Many,
            Cardinality::One,
        ),
    ];
    let link_types: Vec<LinkType> = link_defs
        .iter()
        .map(|(api, display, src, tgt, sc, tc)| LinkType {
            id: rng.uuid(),
            api_name: api.to_string(),
            display_name: display.to_string(),
            description: String::new(),
            source_type_id: by_api(src).id.clone(),
            target_type_id: by_api(tgt).id.clone(),
            source_cardinality: *sc,
            target_cardinality: *tc,
        })
        .collect();
    let link_id = |api: &str| {
        link_types
            .iter()
            .find(|l| l.api_name == api)
            .unwrap()
            .id
            .clone()
    };

    // ---- action types ----------------------------------------------------
    let action_defs: [(&str, &str, &str, Vec<PropertyDef>); 3] = [
        (
            "DELAY_SHIPMENT",
            "Delay shipment",
            "Shipment",
            vec![
                prop("reason", "Reason", DataType::String, true),
                prop("new_eta", "New ETA", DataType::Date, true),
            ],
        ),
        (
            "ASSIGN_VESSEL",
            "Assign vessel",
            "Shipment",
            vec![prop("vessel_imo", "Vessel IMO", DataType::String, true)],
        ),
        (
            "QUARANTINE_SUPPLIER",
            "Quarantine supplier",
            "Supplier",
            vec![prop("note", "Note", DataType::String, false)],
        ),
    ];
    let action_types: Vec<ActionType> = action_defs
        .iter()
        .map(|(api, display, on, params)| ActionType {
            id: rng.uuid(),
            api_name: api.to_string(),
            display_name: display.to_string(),
            description: String::new(),
            object_type_id: by_api(on).id.clone(),
            parameters: params.clone(),
        })
        .collect();

    // ---- instances --------------------------------------------------------
    let ports: Vec<Obj> = [
        ("USLAX", "Los Angeles", "US", 33.74, -118.27, 8),
        ("CNSHA", "Shanghai Yangshan", "CN", 30.62, 122.07, 15),
        ("DEHAM", "Hamburg", "DE", 53.54, 9.93, 9),
        ("SGSIN", "Singapore", "SG", 1.26, 103.83, 13),
        ("NLRTM", "Rotterdam", "NL", 51.95, 4.14, 11),
        ("JPTYO", "Tokyo", "JP", 35.62, 139.78, 7),
    ]
    .iter()
    .map(|(code, name, country, lat, lon, berths)| {
        obj(
            &mut rng,
            &by_api("Port").id.clone(),
            format!("{name} ({code})"),
            props(vec![
                ("port_code", json!(code)),
                ("name", json!(name)),
                ("country", json!(country)),
                ("latitude", json!(lat)),
                ("longitude", json!(lon)),
                ("berth_count", json!(berths)),
            ]),
        )
    })
    .collect();

    let vessel_names = [
        "MV Meridian",
        "MSC Aurora",
        "SS Kestrel",
        "MV Northgale",
        "Ever Lucida",
        "MV Ironclad",
        "SS Palisade",
        "MV Cassandra",
    ];
    let flags = ["PA", "LR", "MH", "SG", "MT", "PA", "LR", "MH"];
    let vessel_statuses = ["AT_SEA", "AT_SEA", "IN_PORT", "MAINTENANCE"];
    let vessels: Vec<Obj> = vessel_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let imo = format!("9{}", rng.int(100000, 999999));
            let cap = rng.int(8, 24) * 1000;
            let status = rng.pick(&vessel_statuses).to_string();
            obj(
                &mut rng,
                &by_api("Vessel").id.clone(),
                name.to_string(),
                props(vec![
                    ("imo_number", json!(imo)),
                    ("name", json!(name)),
                    ("flag", json!(flags[i])),
                    ("capacity_teu", json!(cap)),
                    ("status", json!(status)),
                    (
                        "current_port_code",
                        ports[i % ports.len()].properties["port_code"].clone(),
                    ),
                ]),
            )
        })
        .collect();

    let shipment_statuses = [
        "PLANNED",
        "IN_TRANSIT",
        "IN_TRANSIT",
        "DELAYED",
        "DELIVERED",
    ];
    let shipments: Vec<Obj> = (0..10)
        .map(|i| {
            let day = rng.int(10, 28);
            let status = rng.pick(&shipment_statuses).to_string();
            let teu = rng.int(2, 18);
            let eta_day = rng.int(1, 24);
            obj(
                &mut rng,
                &by_api("Shipment").id.clone(),
                format!("SH-261{}", i + 1),
                props(vec![
                    ("shipment_id", json!(format!("SH-261{}", i + 1))),
                    ("status", json!(status)),
                    ("teu", json!(teu)),
                    ("etd", json!(format!("2026-11-{day:02}"))),
                    ("eta", json!(format!("2026-12-{eta_day:02}"))),
                ]),
            )
        })
        .collect();

    let products: Vec<Obj> = [
        (
            "SKU-4410",
            "Cold Chain Module",
            "Life Sciences",
            18.4,
            false,
        ),
        ("SKU-7731", "Cobalt Battery Pack", "Energy", 62.0, true),
        ("SKU-1180", "Textile Loom Head", "Industrial", 240.0, false),
        (
            "SKU-5522",
            "Optical Sensor Array",
            "Electronics",
            4.2,
            false,
        ),
        ("SKU-9903", "Precision Gearbox", "Automotive", 88.5, false),
        ("SKU-3307", "Vaccine Carrier", "Life Sciences", 12.1, false),
        ("SKU-6614", "Lithium Electrolyte", "Energy", 33.8, true),
        ("SKU-2045", "Aircraft Fastener Set", "Aerospace", 7.7, false),
    ]
    .iter()
    .map(|(sku, name, category, weight, hazmat)| {
        obj(
            &mut rng,
            &by_api("Product").id.clone(),
            name.to_string(),
            props(vec![
                ("sku", json!(sku)),
                ("name", json!(name)),
                ("category", json!(category)),
                ("unit_weight_kg", json!(weight)),
                ("hazmat", json!(hazmat)),
            ]),
        )
    })
    .collect();

    let sizes = ["20FT", "40FT", "40HC"];
    let containers: Vec<Obj> = (0..12)
        .map(|i| {
            let size = rng.pick(&sizes).to_string();
            let serial = rng.int(100000, 999999);
            let number = format!("MSKU{}", rng.int(1000000, 9999999));
            let reefer = rng.next() < 0.25;
            let seal = format!("SL-{}", rng.int(10000, 99999));
            obj(
                &mut rng,
                &by_api("Container").id.clone(),
                format!("MSKU {serial} {i}"),
                props(vec![
                    ("container_number", json!(number)),
                    ("size", json!(size)),
                    ("reefer", json!(reefer)),
                    ("seal_number", json!(seal)),
                ]),
            )
        })
        .collect();

    let factories: Vec<Obj> = [
        (
            "FTY-SHN",
            "Shenzhen Assembly",
            "Shenzhen",
            "CN",
            6,
            1_200_000,
        ),
        ("FTY-DAL", "Dallas Fabrication", "Dallas", "US", 4, 800_000),
        ("FTY-HAM", "Hamburg Works", "Hamburg", "DE", 3, 650_000),
        ("FTY-OSA", "Osaka Precision", "Osaka", "JP", 5, 940_000),
        ("FTY-SIN", "Singapore Flex", "Singapore", "SG", 2, 400_000),
    ]
    .iter()
    .map(|(code, name, city, country, lines, cap)| {
        obj(
            &mut rng,
            &by_api("Factory").id.clone(),
            name.to_string(),
            props(vec![
                ("factory_code", json!(code)),
                ("name", json!(name)),
                ("city", json!(city)),
                ("country", json!(country)),
                ("lines", json!(lines)),
                ("annual_capacity", json!(cap)),
            ]),
        )
    })
    .collect();

    let suppliers: Vec<Obj> = [
        ("TX-88201", "Helix Materials", 1, "DE", 0.12),
        ("TX-10442", "Grand Metro Components", 2, "CN", 0.34),
        ("TX-77310", "Northline Logistics", 2, "US", 0.41),
        ("TX-45509", "Kaiyo Marine Supply", 1, "JP", 0.19),
    ]
    .iter()
    .map(|(tax, name, tier, country, risk)| {
        obj(
            &mut rng,
            &by_api("Supplier").id.clone(),
            name.to_string(),
            props(vec![
                ("tax_id", json!(tax)),
                ("name", json!(name)),
                ("tier", json!(tier)),
                ("country", json!(country)),
                ("risk_score", json!(risk)),
            ]),
        )
    })
    .collect();

    let customers: Vec<Obj> = [
        ("ACC-1001", "Meridian Retail Group", "RETAIL", "NA"),
        ("ACC-1002", "Hansa Industrial", "INDUSTRIAL", "EU"),
        ("ACC-1003", "Kanto Online", "ONLINE", "APAC"),
        ("ACC-1004", "Baltic Wholesale", "WHOLESALE", "EU"),
        ("ACC-1005", "Sunbelt Systems", "INDUSTRIAL", "NA"),
    ]
    .iter()
    .map(|(acc, name, seg, region)| {
        obj(
            &mut rng,
            &by_api("Customer").id.clone(),
            name.to_string(),
            props(vec![
                ("account_id", json!(acc)),
                ("name", json!(name)),
                ("segment", json!(seg)),
                ("region", json!(region)),
            ]),
        )
    })
    .collect();

    let channels = ["DIRECT", "PORTAL", "PARTNER"];
    let orders: Vec<Obj> = (0..8)
        .map(|i| {
            let channel = rng.pick(&channels).to_string();
            let (d, h, m) = (rng.int(1, 9), rng.int(10, 18), rng.int(10, 59));
            let total = rng.dec(4_000.0, 180_000.0);
            obj(
                &mut rng,
                &by_api("SalesOrder").id.clone(),
                format!("SO-88{}", 100 + i),
                props(vec![
                    ("order_number", json!(format!("SO-88{}", 100 + i))),
                    ("channel", json!(channel)),
                    (
                        "placed_at",
                        json!(format!("2026-11-0{d}T{h:02}:{m:02}:00Z")),
                    ),
                    ("total", json!(total)),
                    ("currency", json!("USD")),
                ]),
            )
        })
        .collect();

    let mut objects: Vec<Obj> = Vec::new();
    objects.extend(ports.iter().cloned());
    objects.extend(vessels.iter().cloned());
    objects.extend(shipments.iter().cloned());
    objects.extend(products.iter().cloned());
    objects.extend(containers.iter().cloned());
    objects.extend(factories.iter().cloned());
    objects.extend(suppliers.iter().cloned());
    objects.extend(customers.iter().cloned());
    objects.extend(orders.iter().cloned());

    // ---- instance links ----------------------------------------------------
    let mut seen: HashSet<String> = HashSet::new();
    let mut object_links: Vec<ObjectLink> = vec![];
    let mut push = |rng: &mut Rng, api: &str, s: &Obj, t: &Obj| {
        if seen.insert(format!("{api}:{}:{}", s.id, t.id)) {
            object_links.push(ObjectLink {
                id: rng.uuid(),
                link_type_id: link_id(api),
                source_object_id: s.id.clone(),
                target_object_id: t.id.clone(),
            });
        }
    };

    for v in &vessels {
        let p = ports
            .iter()
            .find(|p| p.properties["port_code"] == v.properties["current_port_code"])
            .unwrap();
        push(&mut rng, "CURRENT_PORT", v, p);
    }
    for s in &shipments {
        let v = rng.pick(&vessels);
        push(&mut rng, "CARRIED_BY", s, v);
    }
    for c in &containers {
        let s = rng.pick(&shipments);
        let p = rng.pick(&products);
        push(&mut rng, "PART_OF_SHIPMENT", c, s);
        push(&mut rng, "HOLDS_PRODUCT", c, p);
    }
    for f in &factories {
        // 3 distinct products per factory
        let mut idx: Vec<usize> = (0..products.len()).collect();
        for i in (1..idx.len()).rev() {
            let j = (rng.next() * (i + 1) as f64) as usize;
            idx.swap(i, j);
        }
        for &pi in idx.iter().take(3) {
            push(&mut rng, "PRODUCES", f, &products[pi]);
        }
    }
    for s in &suppliers {
        let mut idx: Vec<usize> = (0..factories.len()).collect();
        for i in (1..idx.len()).rev() {
            let j = (rng.next() * (i + 1) as f64) as usize;
            idx.swap(i, j);
        }
        for &fi in idx.iter().take(2) {
            push(&mut rng, "SUPPLIES", s, &factories[fi]);
        }
    }
    for o in &orders {
        let cust = rng.pick(&customers);
        push(&mut rng, "PLACED_BY", o, cust);
        let n = rng.int(1, 3) as usize;
        let mut idx: Vec<usize> = (0..products.len()).collect();
        for i in (1..idx.len()).rev() {
            let j = (rng.next() * (i + 1) as f64) as usize;
            idx.swap(i, j);
        }
        for &pi in idx.iter().take(n) {
            push(&mut rng, "CONTAINS_PRODUCT", o, &products[pi]);
        }
    }

    Store::from_state(State {
        object_types: types,
        link_types,
        action_types,
        objects,
        object_links,
        layout: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_deterministic_and_valid() {
        let a = supply_chain();
        let b = supply_chain();
        assert_eq!(
            serde_json::to_value(&a.state).unwrap(),
            serde_json::to_value(&b.state).unwrap()
        );
        assert_eq!(a.state.object_types.len(), 9);
        assert_eq!(a.state.link_types.len(), 10);
        assert_eq!(a.state.action_types.len(), 3);
        assert_eq!(a.state.objects.len(), 66);
        assert!(a.state.object_links.len() > 60);
        // every instance link resolves
        for l in &a.state.object_links {
            assert!(a.link_type(&l.link_type_id).is_some());
            assert!(a.object(&l.source_object_id).is_some());
            assert!(a.object(&l.target_object_id).is_some());
        }
    }
}
