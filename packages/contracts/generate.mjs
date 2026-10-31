// Dependency-free, deterministic JSON Schema -> TypeScript and Rust wire types.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = dirname(fileURLToPath(import.meta.url));
const files = readdirSync(join(root, 'schemas')).filter(n => n.endsWith('.schema.json')).sort();
const schemas = new Map(files.map(file => [file, JSON.parse(readFileSync(join(root, 'schemas', file), 'utf8'))]));
const modules = ['ir', 'model', 'project'];
const declarations = new Map();
const identifier = /^[A-Za-z_][A-Za-z0-9_]*$/;

function resolve(ref, owner) {
  const [file, fragment] = ref.split('#');
  const targetOwner = file || owner;
  let schema = schemas.get(targetOwner);
  if (!schema) throw Error(`Unknown schema reference ${ref} in ${owner}`);
  for (const key of fragment?.split('/').slice(1) ?? []) schema = schema?.[key];
  if (!schema) throw Error(`Unknown schema reference ${ref} in ${owner}`);
  return { schema, owner: targetOwner };
}

function tsType(schema, owner) {
  if (schema.$ref) {
    const target = resolve(schema.$ref, owner);
    if (!target.schema['x-rust-name']) throw Error(`Unnamed reference ${schema.$ref}`);
    return target.schema['x-rust-name'];
  }
  if (Object.hasOwn(schema, 'const')) return JSON.stringify(schema.const);
  if (schema.enum) return schema.enum.map(v => JSON.stringify(v)).join(' | ');
  if (schema.anyOf) return schema.anyOf.map(s => tsType(s, owner)).join(' | ');
  if (Array.isArray(schema.type)) return schema.type.map(t => tsType({ ...schema, type: t }, owner)).join(' | ');
  switch (schema.type) {
    case 'string': return 'string';
    case 'number': case 'integer': return 'number';
    case 'boolean': return 'boolean';
    case 'null': return 'null';
    case 'array': return `Array<${tsType(schema.items, owner)}>`;
    case 'object': {
      if (!schema.properties) return `Record<string, ${schema.additionalProperties && typeof schema.additionalProperties === 'object' ? tsType(schema.additionalProperties, owner) : 'unknown'}>`;
      return `{\n${Object.entries(schema.properties).map(([key, value]) => `  ${JSON.stringify(key)}${schema.required?.includes(key) ? '' : '?'}: ${tsType(value, owner)};`).join('\n')}\n}`;
    }
    default: throw Error(`Unsupported TS schema in ${owner}: ${JSON.stringify(schema)}`);
  }
}

// Only nullable unions are used by the wire contract. Never silently widen a
// future union to serde_json::Value or discard a branch.
function nonNullable(schema) {
  if (schema.anyOf) {
    const branches = schema.anyOf.filter(s => s.type !== 'null');
    if (branches.length !== 1 || branches.length === schema.anyOf.length) throw Error('Rust supports only nullable anyOf unions');
    return { schema: branches[0], nullable: true };
  }
  if (Array.isArray(schema.type)) {
    const types = schema.type.filter(t => t !== 'null');
    if (types.length !== 1 || types.length === schema.type.length) throw Error('Rust supports only nullable type unions');
    return { schema: { ...schema, type: types[0] }, nullable: true };
  }
  if (schema.enum?.includes(null)) return { schema: { ...schema, enum: schema.enum.filter(v => v !== null) }, nullable: true };
  return { schema, nullable: false };
}

function nativeType(schema) {
  const native = schema['x-rust-type'];
  if (!native) return undefined;
  const kind = schema.type ?? (Number.isInteger(schema.const) ? 'integer' : undefined);
  const allowed = { string: ['String', 'Id'], integer: ['u16', 'u32', 'u64'], number: ['f32', 'f64'] };
  if (!allowed[kind]?.includes(native)) throw Error(`Incompatible native Rust type ${native} for ${kind}`);
  return native === 'Id' ? 'crate::ir::Id' : native;
}

function rustType(original, owner, expand = false) {
  const { schema, nullable } = nonNullable(original);
  let value;
  if (schema.$ref) {
    const target = resolve(schema.$ref, owner);
    value = rustType(target.schema, target.owner);
  } else if (schema['x-rust-name'] && !expand) {
    value = `crate::${schema['x-rust-module']}::${schema['x-rust-name']}`;
  } else if (nativeType(schema)) value = nativeType(schema);
  else if (schema.type === 'string') value = schema.format === 'date-time' ? 'chrono::DateTime<chrono::Utc>' : 'String';
  else if (schema.type === 'integer' || Number.isInteger(schema.const)) value = 'u32';
  else if (schema.type === 'number') value = 'f32';
  else if (schema.type === 'boolean') value = 'bool';
  else if (schema.type === 'array') value = `Vec<${rustType(schema.items, owner)}>`;
  else if (schema.type === 'object' && !schema.properties && typeof schema.additionalProperties === 'object') {
    value = `std::collections::BTreeMap<String, ${rustType(schema.additionalProperties, owner)}>`;
  } else throw Error(`Unsupported or unnamed Rust schema in ${owner}: ${JSON.stringify(schema)}`);
  return nullable ? `Option<${value}>` : value;
}

function rustDeclaration(original, owner) {
  const { schema } = nonNullable(original);
  const name = schema['x-rust-name'];
  const extra = schema['x-rust-derives'] ?? [];
  if (extra.some(d => !['Copy', 'PartialEq', 'Eq', 'PartialOrd', 'Ord', 'Default'].includes(d))) throw Error(`Unsupported derives on ${name}`);
  const derives = `#[derive(${['Debug', 'Clone', 'serde::Serialize', 'serde::Deserialize', ...extra].join(', ')})]`;
  const values = schema.enum ?? (typeof schema.const === 'string' ? [schema.const] : undefined);
  if (values) {
    const variants = new Set();
    return `${derives}\npub enum ${name} {\n${values.map(value => {
      if (typeof value !== 'string') throw Error(`Non-string enum ${name}`);
      const variant = value.toLowerCase().split('_').map(part => part[0]?.toUpperCase() + part.slice(1)).join('');
      if (!identifier.test(variant) || variants.has(variant)) throw Error(`Invalid/colliding variant ${name} ${value}`);
      variants.add(variant);
      return `    #[serde(rename = ${JSON.stringify(value)})]\n    ${variant},`;
    }).join('\n')}\n}`;
  }
  if (schema.type === 'object' && schema.properties) {
    const fields = Object.entries(schema.properties).map(([key, field]) => {
      const fieldName = field['x-rust-field'] ?? key;
      if (!identifier.test(fieldName)) throw Error(`Invalid Rust field ${name}.${fieldName}`);
      let fieldType = rustType(field, owner);
      if (!schema.required?.includes(key) && !fieldType.startsWith('Option<')) fieldType = `Option<${fieldType}>`;
      const attributes = [];
      if (fieldName !== key) attributes.push(`    #[serde(rename = ${JSON.stringify(key)})]`);
      if (field['x-rust-skip-none']) {
        if (!fieldType.startsWith('Option<') || schema.required?.includes(key)) throw Error(`Only optional fields can skip None: ${name}.${key}`);
        attributes.push('    #[serde(skip_serializing_if = "Option::is_none")]');
      }
      if (field['x-rust-default'] === true) attributes.push('    #[serde(default)]');
      else if (typeof field['x-rust-default'] === 'string') {
        if (!identifier.test(field['x-rust-default'])) throw Error(`Invalid Rust default on ${name}.${key}`);
        attributes.push(`    #[serde(default = ${JSON.stringify(field['x-rust-default'])})]`);
      }
      return [...attributes, `    pub ${fieldName}: ${fieldType},`].join('\n');
    });
    return `${derives}\n${schema.additionalProperties === false ? '#[serde(deny_unknown_fields)]\n' : ''}pub struct ${name} {\n${fields.join('\n')}\n}`;
  }
  return `pub type ${name} = ${rustType(schema, owner, true)};`;
}

function collect(schema, owner) {
  if (!schema || typeof schema !== 'object') return;
  if (schema['x-rust-name']) {
    const name = schema['x-rust-name'];
    const module = schema['x-rust-module'];
    if (!identifier.test(name) || !modules.includes(module)) throw Error(`Invalid Rust declaration ${module}::${name}`);
    const declaration = rustDeclaration(schema, owner);
    const prior = declarations.get(name);
    if (prior && (prior.declaration !== declaration || prior.module !== module)) throw Error(`Conflicting Rust declaration ${name}`);
    if (!prior) declarations.set(name, { declaration, module });
  }
  for (const value of Object.values(schema)) {
    if (Array.isArray(value)) value.forEach(v => collect(v, owner));
    else if (value && typeof value === 'object') collect(value, owner);
  }
}
for (const [file, schema] of schemas) collect(schema, file);

const sha = content => createHash('sha256').update(content).digest('hex');
const schemaBytes = files.map(file => `${file}\n${readFileSync(join(root, 'schemas', file), 'utf8')}\n`).join('');
const banner = `// Generated by node packages/contracts/generate.mjs; do not edit.\n// Schema SHA-256: ${sha(schemaBytes)}\n\n`;
let output = banner;
for (const [file, schema] of schemas) {
  output += `export type ${schema['x-rust-name']} = ${tsType(schema, file)};\n\n`;
  for (const definition of Object.values(schema.$defs ?? {})) {
    if (!definition['x-rust-name']) throw Error(`Missing export name in ${file}`);
    output += `export type ${definition['x-rust-name']} = ${tsType(definition, file)};\n\n`;
  }
}
output = output.trimEnd() + '\n';
const outputs = new Map([['types.ts', output]]);
for (const module of modules) {
  outputs.set(`${module}.rs`, banner + [...declarations.values()].filter(d => d.module === module).map(d => d.declaration).join('\n\n') + '\n');
}
outputs.set('manifest.json', JSON.stringify({
  schema_sha256: sha(schemaBytes),
  generator_sha256: sha(readFileSync(fileURLToPath(import.meta.url))),
  types_sha256: sha(output),
  rust_sha256: Object.fromEntries(modules.map(module => [`${module}.rs`, sha(outputs.get(`${module}.rs`))])),
}, null, 2) + '\n');
for (const [file, content] of outputs) {
  const path = join(root, 'generated', file);
  if (process.argv.includes('--check')) {
    let actual;
    try { actual = readFileSync(path, 'utf8'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (actual !== content) { process.stderr.write(`Contract drift: ${path}\n`); process.exitCode = 1; }
  } else writeFileSync(path, content);
}
