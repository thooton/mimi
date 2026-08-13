<?php

namespace MediaWiki\Extension\MimiIncubator;

/** Small, dependency-free validator for the JSON Schema keywords used by Mimi. */
final class SchemaValidator {
	/** @return string[] */
	public function validate( mixed $value, object $schema, string $path = '$' ): array {
		$errors = [];
		if ( property_exists( $schema, 'const' ) && $value !== $schema->const ) {
			$errors[] = "$path must equal " . json_encode( $schema->const );
			return $errors;
		}
		if ( isset( $schema->type ) && !$this->hasType( $value, $schema->type ) ) {
			return [ "$path must be {$schema->type}" ];
		}
		if ( is_string( $value ) ) {
			$length = mb_strlen( $value );
			if ( isset( $schema->minLength ) && $length < $schema->minLength ) {
				$errors[] = "$path must not be empty";
			}
			if ( isset( $schema->maxLength ) && $length > $schema->maxLength ) {
				$errors[] = "$path is longer than {$schema->maxLength} characters";
			}
			if ( isset( $schema->pattern ) && !preg_match( '/' . str_replace( '/', '\\/', $schema->pattern ) . '/u', $value ) ) {
				$errors[] = "$path has an invalid format";
			}
		}
		if ( is_int( $value ) && isset( $schema->minimum ) && $value < $schema->minimum ) {
			$errors[] = "$path must be at least {$schema->minimum}";
		}
		if ( is_int( $value ) && isset( $schema->maximum ) && $value > $schema->maximum ) {
			$errors[] = "$path must be at most {$schema->maximum}";
		}
		if ( is_array( $value ) ) {
			$count = count( $value );
			if ( isset( $schema->minItems ) && $count < $schema->minItems ) {
				$errors[] = "$path needs at least {$schema->minItems} item(s)";
			}
			if ( isset( $schema->maxItems ) && $count > $schema->maxItems ) {
				$errors[] = "$path allows at most {$schema->maxItems} item(s)";
			}
			if ( !empty( $schema->uniqueItems ) ) {
				$encoded = array_map( static fn ( $item ) => json_encode( $item ), $value );
				if ( count( $encoded ) !== count( array_unique( $encoded ) ) ) {
					$errors[] = "$path contains duplicate items";
				}
			}
			if ( isset( $schema->items ) ) {
				foreach ( $value as $index => $item ) {
					$errors = array_merge( $errors, $this->validate( $item, $schema->items, "{$path}[$index]" ) );
				}
			}
		}
		if ( is_object( $value ) ) {
			$properties = isset( $schema->properties ) ? get_object_vars( $schema->properties ) : [];
			foreach ( $schema->required ?? [] as $required ) {
				if ( !property_exists( $value, $required ) ) {
					$errors[] = "$path.$required is required";
				}
			}
			if ( isset( $schema->additionalProperties ) && $schema->additionalProperties === false ) {
				foreach ( array_keys( get_object_vars( $value ) ) as $key ) {
					if ( !array_key_exists( $key, $properties ) ) {
						$errors[] = "$path.$key is not allowed";
					}
				}
			}
			foreach ( $properties as $key => $childSchema ) {
				if ( property_exists( $value, $key ) ) {
					$errors = array_merge( $errors, $this->validate( $value->$key, $childSchema, "$path.$key" ) );
				}
			}
		}
		return $errors;
	}

	private function hasType( mixed $value, string $type ): bool {
		return match ( $type ) {
			'object' => is_object( $value ),
			'array' => is_array( $value ),
			'string' => is_string( $value ),
			'integer' => is_int( $value ),
			'number' => is_int( $value ) || is_float( $value ),
			'boolean' => is_bool( $value ),
			'null' => $value === null,
			default => false,
		};
	}
}
