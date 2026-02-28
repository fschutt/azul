import re
import sys

filepath = sys.argv[1]
with open(filepath, 'r') as f:
    content = f.read()

# Pattern 1: xxx.dataset = OptionRefAny::Some(...)  →  xxx.set_dataset(OptionRefAny::Some(...))
content = re.sub(
    r'(\w+(?:\[\w+\])?)\.dataset = (OptionRefAny::Some\(.*?\));',
    r'\1.set_dataset(\2);',
    content
)

# Pattern 2: if let OptionRefAny::Some(ref mut ds) = xxx.dataset  →  if let Some(ds) = xxx.get_dataset_mut()  
content = re.sub(
    r'if let OptionRefAny::Some\(ref mut (\w+)\) = (\w+(?:\[\w+\])?)\.dataset',
    r'if let Some(\1) = \2.get_dataset_mut()',
    content
)

# Pattern 3: assert!(matches!(xxx.dataset, OptionRefAny::None))  →  assert!(xxx.get_dataset().is_none())
content = re.sub(
    r'assert!\(matches!\((\w+(?:\[\w+\])?)\.dataset, OptionRefAny::None\)\)',
    r'assert!(\1.get_dataset().is_none())',
    content
)

with open(filepath, 'w') as f:
    f.write(content)
print(f'Fixed {filepath}')
