use super::Fixup;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Register {
    pub(crate) code: u8,
    pub(crate) width: u8,
    pub(crate) vector: bool,
}
#[derive(Debug)]
pub(crate) enum Operand {
    Register(Register),
    Immediate(i128),
    Memory {
        base: Register,
        index: Option<Register>,
        scale: u8,
        displacement: i32,
    },
    Relative(String),
    Label(String),
}

pub(super) fn number(text: &str) -> Result<i128, String> {
    if let Some(hex) = text.strip_prefix("0x") {
        i128::from_str_radix(hex, 16)
    } else {
        text.parse()
    }
    .map_err(|_| format!("invalid number {text}"))
}

fn register(text: &str) -> Result<Register, String> {
    let name = text.strip_prefix('%').ok_or("register requires %")?;
    if let Some(index) = name.strip_prefix("xmm") {
        let code: u8 = index.parse().map_err(|_| "invalid vector register")?;
        if code < 16 {
            return Ok(Register {
                code,
                width: 128,
                vector: true,
            });
        }
    }
    for (width, names) in [
        (64, ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"]),
        (32, ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"]),
        (8, ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"]),
    ] {
        if let Some(code) = names.iter().position(|&n| n == name) {
            return Ok(Register {
                code: code as u8,
                width,
                vector: false,
            });
        }
    }
    if let Some(index) = name.strip_prefix('r') {
        let (index, width) = if let Some(i) = index.strip_suffix('d') {
            (i, 32)
        } else if let Some(i) = index.strip_suffix('b') {
            (i, 8)
        } else {
            (index, 64)
        };
        if let Ok(code @ 8..=15) = index.parse::<u8>() {
            return Ok(Register {
                code,
                width,
                vector: false,
            });
        }
    }
    Err(format!("unsupported register {text}"))
}

fn operand(text: &str) -> Result<Operand, String> {
    if text.starts_with('%') {
        return register(text).map(Operand::Register);
    }
    if let Some(value) = text.strip_prefix('$') {
        return number(value).map(Operand::Immediate);
    }
    if let Some(label) = text.strip_suffix("(%rip)") {
        return Ok(Operand::Relative(label.into()));
    }
    if let Some((displacement, address)) = text.split_once('(') {
        let parts: Vec<_> = address
            .strip_suffix(')')
            .ok_or("unclosed address")?
            .split(',')
            .collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err("unsupported address".into());
        }
        let base = register(parts[0])?;
        let index = parts.get(1).map(|s| register(s)).transpose()?;
        if base.vector
            || base.width != 64
            || index.is_some_and(|r| r.vector || r.width != 64 || r.code == 4)
        {
            return Err("invalid address registers".into());
        }
        let scale = match parts.get(2).copied().unwrap_or("1") {
            "1" => 0,
            "2" => 1,
            "4" => 2,
            "8" => 3,
            _ => return Err("invalid address scale".into()),
        };
        let displacement = if displacement.is_empty() {
            0
        } else {
            i32::try_from(number(displacement)?).map_err(|_| "displacement exceeds 32 bits")?
        };
        return Ok(Operand::Memory {
            base,
            index,
            scale,
            displacement,
        });
    }
    if text.is_empty() {
        return Err("empty operand".into());
    }
    Ok(Operand::Label(text.into()))
}

fn operands(text: &str) -> Result<Vec<Operand>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut depth = 0i32;
    let mut start = 0;
    let mut result = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(operand(text[start..i].trim())?);
                start = i + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err("invalid parentheses".into());
        }
    }
    if depth != 0 {
        return Err("unclosed parentheses".into());
    }
    result.push(operand(text[start..].trim())?);
    Ok(result)
}

fn reg(operand: &Operand) -> Result<Register, String> {
    if let Operand::Register(r) = operand {
        Ok(*r)
    } else {
        Err("expected register".into())
    }
}

// prefix, REX, opcode, ModR/M, SIB, displacementの順で格納します。
fn modrm(
    prefix: &[u8],
    op: &[u8],
    wide: bool,
    field: u8,
    rm: &Operand,
    byte_rex: bool,
) -> Result<(Vec<u8>, Option<Fixup>), String> {
    let (b, x) = match rm {
        Operand::Register(r) => (r.code >> 3, 0),
        Operand::Memory { base, index, .. } => (base.code >> 3, index.map_or(0, |r| r.code >> 3)),
        Operand::Relative(_) => (0, 0),
        _ => return Err("expected register or memory".into()),
    };
    let rex = 0x40 | (u8::from(wide) << 3) | ((field >> 3) << 2) | (x << 1) | b;
    let mut bytes = prefix.to_vec();
    if rex != 0x40 || byte_rex {
        bytes.push(rex);
    }
    bytes.extend_from_slice(op);
    let bits = (field & 7) << 3;
    let mut fixup = None;
    match rm {
        Operand::Register(r) => bytes.push(0xc0 | bits | (r.code & 7)),
        Operand::Relative(symbol) => {
            bytes.push(bits | 5);
            fixup = Some(Fixup {
                offset: bytes.len(),
                symbol: symbol.clone(),
                addend: 0,
                call: false,
            });
            bytes.extend([0; 4]);
        }
        Operand::Memory {
            base,
            index,
            scale,
            displacement,
        } => {
            let mode = if *displacement == 0 && base.code & 7 != 5 {
                0
            } else if i8::try_from(*displacement).is_ok() {
                1
            } else {
                2
            };
            let sib = index.is_some() || base.code & 7 == 4;
            bytes.push((mode << 6) | bits | if sib { 4 } else { base.code & 7 });
            if sib {
                bytes
                    .push((*scale << 6) | (index.map_or(4, |r| r.code & 7) << 3) | (base.code & 7));
            }
            if mode == 1 {
                bytes.push(*displacement as u8);
            } else if mode == 2 {
                bytes.extend(displacement.to_le_bytes());
            }
        }
        _ => unreachable!(),
    }
    Ok((bytes, fixup))
}

fn signed32(value: i128) -> Result<[u8; 4], String> {
    i32::try_from(value)
        .map(i32::to_le_bytes)
        .map_err(|_| "immediate exceeds signed 32 bits".into())
}

fn condition(name: &str) -> Option<u8> {
    Some(match name {
        "o" => 0,
        "no" => 1,
        "b" | "c" => 2,
        "ae" => 3,
        "e" => 4,
        "ne" => 5,
        "be" => 6,
        "a" => 7,
        "s" => 8,
        "ns" => 9,
        "p" => 10,
        "np" => 11,
        "l" => 12,
        "ge" => 13,
        "le" => 14,
        "g" => 15,
        _ => return None,
    })
}

pub(super) fn instruction(line: &str) -> Result<(Vec<u8>, Option<Fixup>), String> {
    let (name, args) = line.split_once(' ').unwrap_or((line, ""));
    let raw = args.trim();
    let indirect = name == "callq" && raw.starts_with('*');
    let args = operands(if indirect { &raw[1..] } else { raw })?;
    if name == "callq" && indirect == matches!(args.as_slice(), [Operand::Label(_)]) {
        return Err("callq requires a label or an explicit indirect operand".into());
    }
    instruction_operands(name, &args)
}

pub(crate) fn instruction_operands(
    name: &str,
    args: &[Operand],
) -> Result<(Vec<u8>, Option<Fixup>), String> {
    validate(name, args)?;
    let mut result = encode(name, args)?;
    if let Some(fixup) = &mut result.1 {
        fixup.addend = fixup.offset as i64 - result.0.len() as i64;
    }
    if result.0.len() > 15 {
        return Err("instruction exceeds x86 length limit".into());
    }
    Ok(result)
}

// 部分対応を誤った符号化として受け入れないよう、レジスタ種別と幅を先に検査します。
fn validate(name: &str, args: &[Operand]) -> Result<(), String> {
    let gp =
        |op: &Operand, width| matches!(op, Operand::Register(r) if !r.vector && r.width == width);
    let xmm = |op: &Operand| matches!(op, Operand::Register(r) if r.vector);
    let memory = |op: &Operand| matches!(op, Operand::Memory { .. } | Operand::Relative(_));
    let immediate = |op: &Operand| matches!(op, Operand::Immediate(_));
    let valid = if let [a, b] = args {
        match name {
            "movabsq" => immediate(a) && gp(b, 64),
            "movzbq" | "movzbl" => {
                (gp(a, 8) || memory(a)) && gp(b, if name == "movzbq" { 64 } else { 32 })
            }
            "leaq" => memory(a) && gp(b, 64),
            "cvtsi2ssq" | "cvtsi2sdq" => (gp(a, 64) || memory(a)) && xmm(b),
            "cvttsd2siq" => (xmm(a) || memory(a)) && gp(b, 64),
            "movq" if xmm(a) || xmm(b) => {
                (xmm(a) && (gp(b, 64) || memory(b))) || ((gp(a, 64) || memory(a)) && xmm(b))
            }
            "movss" | "movsd" | "movaps" | "movapd" => {
                (xmm(a) && (xmm(b) || memory(b))) || (memory(a) && xmm(b))
            }
            "addss" | "addsd" | "subss" | "subsd" | "mulss" | "mulsd" | "divss" | "divsd"
            | "ucomiss" | "ucomisd" | "xorps" | "xorpd" | "cvtss2sd" | "cvtsd2ss" => {
                (xmm(a) || memory(a)) && xmm(b)
            }
            "shlq" | "shrq" | "sarq" | "btcq" => {
                (immediate(a) || gp(a, 8)) && (gp(b, 64) || memory(b))
            }
            _ => {
                let width = if name.ends_with('b') {
                    8
                } else if name.ends_with('l') {
                    32
                } else {
                    64
                };
                (gp(a, width) || memory(a) || immediate(a))
                    && (gp(b, width) || memory(b))
                    && !(memory(a) && memory(b))
            }
        }
    } else if let [Operand::Immediate(_), a, b] = args {
        name == "imulq" && (gp(a, 64) || memory(a)) && gp(b, 64)
    } else if let [op] = args {
        if name == "int" {
            matches!(op, Operand::Immediate(0x29))
        } else if name.starts_with("set") {
            gp(op, 8)
        } else {
            gp(op, 64) || memory(op) || matches!(op, Operand::Label(_))
        }
    } else {
        args.is_empty()
    };
    if valid {
        Ok(())
    } else {
        Err(format!("unsupported operand types for {name}"))
    }
}

fn encode(name: &str, args: &[Operand]) -> Result<(Vec<u8>, Option<Fixup>), String> {
    if name == "int" && matches!(args, [Operand::Immediate(0x29)]) {
        return Ok((vec![0xcd, 0x29], None));
    }
    if args.is_empty() {
        return Ok((
            match name {
                "retq" => vec![0xc3],
                "cqto" => vec![0x48, 0x99],
                "ud2" => vec![0x0f, 0x0b],
                _ => return Err(format!("unsupported instruction {name}")),
            },
            None,
        ));
    }
    if let [Operand::Label(symbol)] = args {
        let mut bytes = if name == "callq" {
            vec![0xe8]
        } else if name == "jmp" {
            vec![0xe9]
        } else if let Some(code) = name.strip_prefix('j').and_then(condition) {
            vec![0x0f, 0x80 | code]
        } else {
            return Err("unsupported label instruction".into());
        };
        let fixup = Fixup {
            offset: bytes.len(),
            symbol: symbol.clone(),
            addend: 0,
            call: name == "callq",
        };
        bytes.extend([0; 4]);
        return Ok((bytes, Some(fixup)));
    }
    if let [operand] = args {
        if name == "callq" {
            return modrm(&[], &[0xff], false, 2, operand, false);
        }
        if name == "pushq" || name == "popq" {
            let r = reg(operand)?;
            if r.vector || r.width != 64 {
                return Err("invalid push/pop register".into());
            }
            let mut b = Vec::new();
            if r.code >= 8 {
                b.push(0x41);
            }
            b.push(if name == "pushq" { 0x50 } else { 0x58 } + (r.code & 7));
            return Ok((b, None));
        }
        if let Some(code) = name.strip_prefix("set").and_then(condition) {
            let r = reg(operand)?;
            if r.vector || r.width != 8 {
                return Err("setcc requires byte register".into());
            }
            return modrm(&[], &[0x0f, 0x90 | code], false, 0, operand, r.code >= 4);
        }
        let (opcode, extension) = match name {
            "negq" => (0xf7, 3),
            "mulq" => (0xf7, 4),
            "divq" => (0xf7, 6),
            "idivq" => (0xf7, 7),
            "incq" => (0xff, 0),
            _ => return Err(format!("unsupported unary instruction {name}")),
        };
        return modrm(&[], &[opcode], true, extension, operand, false);
    }
    if let [Operand::Immediate(value), source, destination] = args {
        if name != "imulq" {
            return Err("unsupported ternary instruction".into());
        }
        let r = reg(destination)?;
        let mut result = modrm(&[], &[0x69], true, r.code, source, false)?;
        result.0.extend(signed32(*value)?);
        return Ok(result);
    }
    let [source, destination] = args else {
        return Err("unsupported operand count".into());
    };
    if name == "movb" {
        if let Operand::Immediate(value) = source {
            if !(-128..=255).contains(value) {
                return Err("byte move immediate out of range".into());
            }
            if let Operand::Register(r) = destination {
                let mut bytes = Vec::new();
                if r.code >= 4 {
                    bytes.push(0x40 | (r.code >> 3));
                }
                bytes.extend([0xb0 | (r.code & 7), *value as u8]);
                return Ok((bytes, None));
            }
            let mut result = modrm(&[], &[0xc6], false, 0, destination, false)?;
            result.0.push(*value as u8);
            return Ok(result);
        }
        if let Operand::Register(r) = source {
            return modrm(
                &[],
                &[0x88],
                false,
                r.code,
                destination,
                r.code >= 4 || matches!(destination, Operand::Register(d) if d.code >= 4),
            );
        }
        let r = reg(destination)?;
        return modrm(&[], &[0x8a], false, r.code, source, r.code >= 4);
    }
    if name == "movq"
        && (matches!(source,Operand::Register(r) if r.vector)
            || matches!(destination,Operand::Register(r) if r.vector))
    {
        if let Operand::Register(r) = source
            && r.vector
        {
            return modrm(&[0x66], &[0x0f, 0x7e], true, r.code, destination, false);
        }
        return modrm(
            &[0x66],
            &[0x0f, 0x6e],
            true,
            reg(destination)?.code,
            source,
            false,
        );
    }
    if matches!(name, "movq" | "movabsq" | "movl") {
        let wide = name != "movl";
        if let Operand::Immediate(value) = source {
            if let Operand::Register(r) = destination {
                if r.vector || r.width != if wide { 64 } else { 32 } {
                    return Err("invalid move register width".into());
                }
                let mut b = Vec::new();
                if wide {
                    b.push(0x48 | (r.code >> 3));
                } else if r.code >= 8 {
                    b.push(0x41);
                }
                b.push(0xb8 | (r.code & 7));
                let width = if wide { 8 } else { 4 };
                if *value < -(1i128 << (width * 8 - 1)) || *value >= 1i128 << (width * 8) {
                    return Err("move immediate out of range".into());
                }
                b.extend_from_slice(&value.to_le_bytes()[..width]);
                return Ok((b, None));
            }
            let mut result = modrm(&[], &[0xc7], wide, 0, destination, false)?;
            result.0.extend(signed32(*value)?);
            return Ok(result);
        }
        if let Operand::Register(r) = source {
            return modrm(&[], &[0x89], wide, r.code, destination, false);
        }
        return modrm(&[], &[0x8b], wide, reg(destination)?.code, source, false);
    }
    if matches!(name, "movzbq" | "movzbl" | "leaq") {
        return modrm(
            &[],
            if name == "leaq" {
                &[0x8d]
            } else {
                &[0x0f, 0xb6]
            },
            name != "movzbl",
            reg(destination)?.code,
            source,
            matches!(source, Operand::Register(r) if r.width == 8 && r.code >= 4),
        );
    }
    if name == "cmovne" {
        return modrm(
            &[],
            &[0x0f, 0x45],
            true,
            reg(destination)?.code,
            source,
            false,
        );
    }
    let sse = match name {
        "movss" => Some((0xf3, 0x10, false)),
        "movsd" => Some((0xf2, 0x10, false)),
        "movaps" => Some((0, 0x28, false)),
        "movapd" => Some((0x66, 0x28, false)),
        "xorps" => Some((0, 0x57, false)),
        "xorpd" => Some((0x66, 0x57, false)),
        "ucomiss" => Some((0, 0x2e, false)),
        "ucomisd" => Some((0x66, 0x2e, false)),
        "addss" => Some((0xf3, 0x58, false)),
        "addsd" => Some((0xf2, 0x58, false)),
        "subss" => Some((0xf3, 0x5c, false)),
        "subsd" => Some((0xf2, 0x5c, false)),
        "mulss" => Some((0xf3, 0x59, false)),
        "mulsd" => Some((0xf2, 0x59, false)),
        "divss" => Some((0xf3, 0x5e, false)),
        "divsd" => Some((0xf2, 0x5e, false)),
        "cvtsi2ssq" => Some((0xf3, 0x2a, true)),
        "cvtsi2sdq" => Some((0xf2, 0x2a, true)),
        "cvttsd2siq" => Some((0xf2, 0x2c, true)),
        "cvtss2sd" => Some((0xf3, 0x5a, false)),
        "cvtsd2ss" => Some((0xf2, 0x5a, false)),
        _ => None,
    };
    if let Some((prefix, opcode, wide)) = sse {
        let prefixes = if prefix == 0 { vec![] } else { vec![prefix] };
        if name.starts_with("mov") && !matches!(destination, Operand::Register(_)) {
            return modrm(
                &prefixes,
                &[0x0f, opcode + 1],
                wide,
                reg(source)?.code,
                destination,
                false,
            );
        }
        return modrm(
            &prefixes,
            &[0x0f, opcode],
            wide,
            reg(destination)?.code,
            source,
            false,
        );
    }
    if matches!(name, "shlq" | "shrq" | "sarq" | "btcq") {
        let extension = match name {
            "shlq" => 4,
            "shrq" => 5,
            "sarq" => 7,
            _ => 7,
        };
        if let Operand::Immediate(value) = source {
            let mut result = modrm(
                &[],
                if name == "btcq" {
                    &[0x0f, 0xba]
                } else {
                    &[0xc1]
                },
                true,
                extension,
                destination,
                false,
            )?;
            result
                .0
                .push(u8::try_from(*value).map_err(|_| "shift immediate out of range")?);
            return Ok(result);
        }
        let r = reg(source)?;
        if name == "btcq" || r.code != 1 || r.width != 8 {
            return Err("shift requires CL".into());
        }
        return modrm(&[], &[0xd3], true, extension, destination, false);
    }
    if name == "imulq" {
        if let Operand::Immediate(value) = source {
            let mut result = modrm(
                &[],
                &[0x69],
                true,
                reg(destination)?.code,
                destination,
                false,
            )?;
            result.0.extend(signed32(*value)?);
            return Ok(result);
        }
        return modrm(
            &[],
            &[0x0f, 0xaf],
            true,
            reg(destination)?.code,
            source,
            false,
        );
    }
    let wide = name.ends_with('q');
    let byte = name.ends_with('b');
    let stem = name
        .strip_suffix(['q', 'l', 'b'])
        .ok_or_else(|| format!("unsupported instruction {name}"))?;
    let (opcode, extension) = match stem {
        "add" => (0x01, 0),
        "or" => (0x09, 1),
        "and" => (0x21, 4),
        "sub" => (0x29, 5),
        "xor" => (0x31, 6),
        "cmp" => (0x39, 7),
        "test" => (0x85, 0),
        _ => return Err(format!("unsupported instruction {name}")),
    };
    let byte_rex = byte
        && [source, destination]
            .iter()
            .any(|o| matches!(o,Operand::Register(r) if r.code>=4));
    if let Operand::Immediate(value) = source {
        let op = if stem == "test" {
            if byte { 0xf6 } else { 0xf7 }
        } else if byte {
            0x80
        } else {
            0x81
        };
        let mut result = modrm(&[], &[op], wide, extension, destination, byte_rex)?;
        if byte {
            result
                .0
                .push(u8::try_from(*value).map_err(|_| "byte immediate out of range")?);
        } else {
            result.0.extend(signed32(*value)?);
        }
        return Ok(result);
    }
    if let Operand::Register(r) = source {
        return modrm(
            &[],
            &[opcode - u8::from(byte)],
            wide,
            r.code,
            destination,
            byte_rex,
        );
    }
    if stem == "test" {
        return Err("unsupported test operand order".into());
    }
    modrm(
        &[],
        &[opcode + 2 - u8::from(byte)],
        wide,
        reg(destination)?.code,
        source,
        byte_rex,
    )
}
