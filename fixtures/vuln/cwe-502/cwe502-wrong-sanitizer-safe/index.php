<?php

$value = addslashes($_GET['x']);
$object = unserialize($value, ['allowed_classes' => false]);
